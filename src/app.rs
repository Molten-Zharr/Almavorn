use crate::{
    audio::Audio,
    input::{Action, Input, Key, KeyPress},
    media,
    model::{Entry, Language, Mode, Playlist, PlaylistKind, Settings, Track},
    store::{Change, Snapshot, Store},
};
use anyhow::{Context, Result, ensure};
use ratatui::layout::{Position, Rect};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    runtime::{Builder, Runtime},
    sync::oneshot,
};

type Background<T> = oneshot::Receiver<Result<T>>;

fn background<T: Send + 'static>(
    runtime: &Runtime,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Background<T> {
    let (sender, receiver) = oneshot::channel();
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(operation)
            .await
            .context("Background operation failed")
            .and_then(|result| result);
        let _ = sender.send(result);
    });
    receiver
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Playlists,
    Tracks,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Position,
    Title,
    Artist,
    Album,
    Duration,
}

impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Position => Self::Title,
            Self::Title => Self::Artist,
            Self::Artist => Self::Album,
            Self::Album => Self::Duration,
            Self::Duration => Self::Position,
        }
    }
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Position => language.text("Position", "Позиция"),
            Self::Title => language.text("Title", "Название"),
            Self::Artist => language.text("Artist", "Исполнитель"),
            Self::Album => language.text("Album", "Альбом"),
            Self::Duration => language.text("Duration", "Длительность"),
        }
    }
}

#[derive(Clone)]
pub enum TextPurpose {
    Create,
    RenamePlaylist(i64),
    RenameTrack(i64, i64),
    Search,
    DeletePlaylist(i64, String),
    Accent,
}

#[derive(Clone)]
pub struct TextDialog {
    pub purpose: TextPurpose,
    pub text: String,
    pub keyboard: Language,
    pub upper: bool,
    pub selected_all: bool,
}

#[derive(Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub directory: bool,
}
#[derive(Clone)]
pub struct Browser {
    pub directory: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: usize,
    pub offset: usize,
    pub marked: HashSet<PathBuf>,
    pub folder: bool,
}

impl Browser {
    fn open(directory: PathBuf, folder: bool) -> Result<Self> {
        let mut browser = Self {
            directory,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            marked: HashSet::new(),
            folder,
        };
        browser.refresh()?;
        Ok(browser)
    }
    fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        for entry in std::fs::read_dir(&self.directory)?.collect::<std::io::Result<Vec<_>>>()? {
            let directory = entry.path().is_dir();
            if directory || (!self.folder && media::is_audio(&entry.path())) {
                self.entries.push(BrowserEntry {
                    path: entry.path(),
                    directory,
                });
            }
        }
        self.entries.sort_by_key(|entry| {
            (
                !entry.directory,
                entry.path.file_name().unwrap_or_default().to_os_string(),
            )
        });
        self.selected = 0;
        self.offset = 0;
        Ok(())
    }
}

#[derive(Clone)]
pub enum Dialog {
    Text(TextDialog),
    Browser(Browser),
    RemoveEntry {
        playlist: i64,
        entry: i64,
        title: String,
    },
    Transfer {
        ids: Vec<i64>,
        selected: usize,
    },
    Help {
        offset: usize,
    },
    Settings {
        selected: usize,
    },
    Metadata {
        track: Track,
        offset: usize,
    },
    CaptureBinding {
        index: usize,
    },
}

#[derive(Clone)]
pub enum Target {
    Action(Action),
    Mode(Mode),
    Playlist(i64),
    Track(i64),
    Mark(i64),
    Seek(Rect),
    CloseDialog,
    Submit,
    Text(char),
    Backspace,
    KeyboardLanguage,
    KeyboardCase,
    BrowserRow(usize),
    BrowserParent,
    BrowserMarkAll,
    BrowserOpen,
    BrowserAdd,
    TransferRow(usize),
    DialogScroll(i16),
    Setting(usize),
}

pub struct Hit {
    pub area: Rect,
    pub target: Target,
    pub enabled: bool,
}

struct ImportOutcome {
    change: Change,
    errors: Vec<String>,
    skipped: usize,
}
struct ImportJob {
    receiver: Background<ImportOutcome>,
    target: i64,
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
}

enum DatabaseOutcome {
    Changed(Change),
    Created(Change),
    Renamed,
    Restored { change: Change, redo: bool },
}
struct DatabaseJob {
    receiver: Background<DatabaseOutcome>,
    recovery: Option<Dialog>,
    undo: Option<(Change, bool)>,
    selection: Option<i64>,
    mode: Mode,
}
struct LibraryJob {
    receiver: Background<Vec<Playlist>>,
    revision: u64,
}

pub struct App {
    pub store: Store,
    runtime: Option<Runtime>,
    pub settings: Settings,
    pub playlists: Vec<Playlist>,
    pub selected_playlist: Option<i64>,
    pub selected_entry: Option<i64>,
    pub marked: HashSet<i64>,
    pub focus: Focus,
    pub sort: Sort,
    pub query: String,
    pub dialog: Option<Dialog>,
    pub hits: Vec<Hit>,
    pub playlist_area: Rect,
    pub tracks_area: Rect,
    pub playlist_offset: usize,
    pub track_offset: usize,
    pub notice: String,
    pub notice_error: bool,
    pub quit: bool,
    pub editing: bool,
    pub current: Option<Track>,
    queue: Vec<Track>,
    queue_index: usize,
    audio: Option<Audio>,
    import: Option<ImportJob>,
    database: Option<DatabaseJob>,
    library: Option<LibraryJob>,
    library_revision: u64,
    pending_selection: Option<(i64, Mode)>,
    settings_job: Option<Background<()>>,
    pending_settings: Option<Settings>,
    settings_failed: bool,
    histories: HashMap<&'static str, (Vec<Change>, Vec<Change>)>,
    pointer: Position,
}

impl App {
    pub fn new(directory: &Path) -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
            .thread_name("almavorn-worker")
            .enable_all()
            .build()
            .context("Cannot start background workers")?;
        let store = Store::open(directory)?;
        let mut settings = store.settings()?;
        settings.volume = settings.volume.clamp(0.0, 1.0);
        for binding in crate::input::default_bindings() {
            if !settings
                .bindings
                .iter()
                .any(|value| value.action == binding.action)
            {
                settings.bindings.push(binding);
            }
        }
        let playlists = store.playlists()?;
        let selected_playlist = playlists
            .iter()
            .find(|playlist| playlist.mode == settings.mode)
            .map(|playlist| playlist.id);
        let selected_entry = playlists
            .iter()
            .find(|playlist| Some(playlist.id) == selected_playlist)
            .and_then(|playlist| playlist.entries.first())
            .map(|entry| entry.id);
        let language = settings.language;
        let library_revision = store.revision();
        Ok(Self {
            store,
            runtime: Some(runtime),
            settings,
            playlists,
            selected_playlist,
            selected_entry,
            marked: HashSet::new(),
            focus: Focus::Tracks,
            sort: Sort::Position,
            query: String::new(),
            dialog: None,
            hits: Vec::new(),
            playlist_area: Rect::default(),
            tracks_area: Rect::default(),
            playlist_offset: 0,
            track_offset: 0,
            notice: language
                .text(
                    "Ready. Add music or create a playlist. F1: help.",
                    "Готово. Добавьте музыку или создайте плейлист. F1: помощь.",
                )
                .into(),
            notice_error: false,
            quit: false,
            editing: false,
            current: None,
            queue: Vec::new(),
            queue_index: 0,
            audio: None,
            import: None,
            database: None,
            library: None,
            library_revision,
            pending_selection: None,
            settings_job: None,
            pending_settings: None,
            settings_failed: false,
            histories: HashMap::new(),
            pointer: Position::default(),
        })
    }

    pub fn text(&self, english: &'static str, russian: &'static str) -> &'static str {
        self.settings.language.text(english, russian)
    }
    pub fn playlist(&self) -> Option<&Playlist> {
        self.playlists
            .iter()
            .find(|playlist| Some(playlist.id) == self.selected_playlist)
    }
    pub fn visible_playlists(&self) -> Vec<&Playlist> {
        self.playlists
            .iter()
            .filter(|playlist| playlist.mode == self.settings.mode)
            .collect()
    }
    pub fn rows(&self) -> Vec<&Entry> {
        let Some(playlist) = self.playlist() else {
            return Vec::new();
        };
        let query = self.query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut entries: Vec<_> = playlist
            .entries
            .iter()
            .filter(|entry| {
                let text = format!(
                    "{} {} {} {}",
                    entry.track.title,
                    entry.track.artist,
                    entry.track.album,
                    entry.track.path.display()
                )
                .to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .collect();
        entries.sort_by(|a, b| {
            match self.sort {
                Sort::Position => a.position.cmp(&b.position),
                Sort::Title => a
                    .track
                    .title
                    .to_lowercase()
                    .cmp(&b.track.title.to_lowercase()),
                Sort::Artist => a
                    .track
                    .artist
                    .to_lowercase()
                    .cmp(&b.track.artist.to_lowercase()),
                Sort::Album => a
                    .track
                    .album
                    .to_lowercase()
                    .cmp(&b.track.album.to_lowercase()),
                Sort::Duration => a.track.duration_ms.cmp(&b.track.duration_ms),
            }
            .then_with(|| a.position.cmp(&b.position))
        });
        entries
    }
    pub fn entry(&self) -> Option<&Entry> {
        self.playlist()?
            .entries
            .iter()
            .find(|entry| Some(entry.id) == self.selected_entry)
    }
    pub fn busy(&self) -> bool {
        self.import.is_some() || self.database_busy()
    }
    fn database_busy(&self) -> bool {
        self.database.is_some() || self.library_revision != self.store.revision()
    }
    pub fn importing(&self) -> bool {
        self.import.is_some()
    }
    pub fn progress(&self) -> usize {
        self.import
            .as_ref()
            .map_or(0, |job| job.progress.load(Ordering::Relaxed))
    }
    pub fn position_ms(&self) -> u64 {
        if self.current.is_none() {
            return 0;
        }
        self.audio.as_ref().map_or(0, |audio| {
            audio.position().as_millis().min(u64::MAX as u128) as u64
        })
    }
    pub fn paused(&self) -> bool {
        self.audio.as_ref().is_none_or(Audio::paused) || self.current.is_none()
    }
    pub fn accepts_text(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Text(_)))
    }

    pub fn hovered(&self, area: Rect) -> bool {
        area.contains(self.pointer)
    }

    pub fn allowed(&self, action: Action) -> bool {
        use Action::*;
        if self.database_busy()
            && matches!(
                action,
                AddFiles
                    | AddFolder
                    | AddNew
                    | NewPlaylist
                    | Rename
                    | Delete
                    | MoveUp
                    | MoveDown
                    | Transfer
                    | Undo
                    | Redo
                    | ToggleEdit
                    | ToggleDesk
                    | SwitchMode
            )
        {
            return false;
        }
        if self.importing()
            && matches!(
                action,
                AddFiles | AddFolder | AddNew | ToggleEdit | ToggleDesk | SwitchMode
            )
        {
            return false;
        }
        let editable = self
            .playlist()
            .is_some_and(|playlist| playlist.can_edit(self.editing));
        match action {
            ToggleEdit => self.settings.mode == Mode::Order,
            Rename | Delete => {
                editable
                    && match self.focus {
                        Focus::Playlists => self
                            .playlist()
                            .is_some_and(|playlist| playlist.kind == PlaylistKind::Normal),
                        Focus::Tracks => self.entry().is_some_and(|entry| {
                            action == Delete
                                || self.editing
                                || !self.playlists.iter().any(|playlist| {
                                    playlist.mode == Mode::Order
                                        && playlist.kind == PlaylistKind::Normal
                                        && playlist
                                            .entries
                                            .iter()
                                            .any(|other| other.track.id == entry.track.id)
                                })
                        }),
                    }
            }
            MoveUp | MoveDown => {
                if !editable {
                    return false;
                }
                let (index, length) = if self.focus == Focus::Playlists {
                    let lists: Vec<_> = self
                        .visible_playlists()
                        .into_iter()
                        .filter(|playlist| playlist.kind == PlaylistKind::Normal)
                        .collect();
                    (
                        lists
                            .iter()
                            .position(|playlist| Some(playlist.id) == self.selected_playlist),
                        lists.len(),
                    )
                } else {
                    if self.sort != crate::app::Sort::Position || !self.query.is_empty() {
                        return false;
                    }
                    self.playlist()
                        .map(|playlist| {
                            (
                                playlist
                                    .entries
                                    .iter()
                                    .position(|entry| Some(entry.id) == self.selected_entry),
                                playlist.entries.len(),
                            )
                        })
                        .unwrap_or((None, 0))
                };
                index.is_some_and(|index| {
                    if action == MoveUp {
                        index > 0
                    } else {
                        index + 1 < length
                    }
                })
            }
            TogglePlay => self.current.is_some() || self.entry().is_some(),
            Next => self.current.is_some() && self.queue_index + 1 < self.queue.len(),
            Previous => self.current.is_some() && self.queue_index > 0,
            SeekForward | SeekBackward | Stop => self.current.is_some(),
            VolumeUp => self.settings.volume < 1.0,
            VolumeDown => self.settings.volume > 0.0,
            Transfer | Metadata | Mark => self.entry().is_some(),
            AddFiles | AddFolder => self.import_target().is_some(),
            AddNew => {
                self.import_target().is_some()
                    && self
                        .playlist()
                        .is_some_and(|playlist| !playlist.entries.is_empty())
            }
            Undo | Redo => self.scope().is_some_and(|scope| {
                self.histories.get(scope).is_some_and(|(undo, redo)| {
                    if action == Undo {
                        !undo.is_empty()
                    } else {
                        !redo.is_empty()
                    }
                })
            }),
            _ => true,
        }
    }

    fn import_target(&self) -> Option<i64> {
        if self.settings.sorting_desk {
            self.playlists
                .iter()
                .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
                .map(|playlist| playlist.id)
        } else {
            self.playlist()
                .filter(|playlist| playlist.can_edit(self.editing))
                .map(|playlist| playlist.id)
        }
    }
    fn scope(&self) -> Option<&'static str> {
        if self
            .playlist()
            .is_some_and(|playlist| playlist.kind == PlaylistKind::SortingDesk)
        {
            Some("desk")
        } else if self.settings.mode == Mode::Chaos {
            Some("chaos")
        } else {
            None
        }
    }
    fn save_settings(&mut self) -> Result<()> {
        self.pending_settings = Some(self.settings.clone());
        self.settings_failed = false;
        self.start_settings()
    }
    fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("runtime is alive while the interface runs")
    }
    fn start_settings(&mut self) -> Result<()> {
        if !self.settings_failed
            && self.settings_job.is_none()
            && let Some(settings) = &self.pending_settings
        {
            let mut store = self.store.try_clone()?;
            let settings = settings.clone();
            self.settings_job = Some(background(self.runtime(), move || {
                store.save_settings(&settings)
            }));
            self.pending_settings = None;
        }
        Ok(())
    }
    fn start_database(
        &mut self,
        recovery: Option<Dialog>,
        operation: impl FnOnce(&mut Store) -> Result<DatabaseOutcome> + Send + 'static,
    ) -> Result<()> {
        ensure!(
            !self.database_busy(),
            "Wait for the current library operation to finish"
        );
        let mut store = self.store.try_clone()?;
        self.database = Some(DatabaseJob {
            receiver: background(self.runtime(), move || operation(&mut store)),
            recovery,
            undo: None,
            selection: self.selected_playlist,
            mode: self.settings.mode,
        });
        Ok(())
    }
    fn message(&mut self, text: String) {
        self.notice = text;
        self.notice_error = false;
    }
    fn failure(&mut self, error: anyhow::Error) {
        let original = error.to_string();
        let russian = match original.as_str() {
            "Enable Order editing before changing this playlist" => {
                Some("Сначала включите редактирование Порядка.")
            }
            "Enable Order editing before renaming tracks" => {
                Some("Для переименования включите редактирование Порядка.")
            }
            "Track is also used by a protected Order playlist" => {
                Some("Композиция используется в защищенном плейлисте Порядка.")
            }
            "Enter the exact playlist name"
            | "Enter the exact playlist name to confirm removal" => {
                Some("Введите точное имя плейлиста для подтверждения.")
            }
            "Name must contain 1–160 printable characters" => {
                Some("Имя должно содержать от 1 до 160 символов.")
            }
            "Playlist no longer exists" => Some("Плейлист больше не существует."),
            "Track no longer exists in this playlist" => {
                Some("Композиции больше нет в этом плейлисте.")
            }
            "Select music files first" => Some("Сначала выберите музыкальные файлы."),
            "Music import is already running" => {
                Some("Добавление музыкальных файлов уже выполняется.")
            }
            "This shortcut is already assigned" => Some("Это сочетание клавиш уже назначено."),
            "Navigation keys are reserved" => Some("Эти клавиши зарезервированы для навигации."),
            "Use a six-digit color, for example E89E4A" => {
                Some("Введите шесть цифр цвета, например E89E4A.")
            }
            "Playlist changed concurrently; undo is no longer safe" => Some(
                "Плейлист изменен другой задачей. Отмена могла бы затронуть более новые изменения.",
            ),
            "Library changed concurrently; no changes were saved. Repeat the action" => Some(
                "Библиотека изменена другой задачей. Изменения не сохранены. Повторите действие.",
            ),
            "Wait for the current library operation to finish" => {
                Some("Дождитесь завершения текущей операции с библиотекой.")
            }
            "Background operation failed" => {
                Some("Фоновая операция завершилась с ошибкой. Повторите действие.")
            }
            "Background operation was interrupted" => {
                Some("Фоновая операция прервана. Повторите действие.")
            }
            "Audio output unavailable" => {
                Some("Устройство вывода звука недоступно. Проверьте подключение и настройки звука.")
            }
            "Seeking is not available for this file" => {
                Some("Для этого файла перемотка недоступна.")
            }
            _ => None,
        };
        let reason = if self.settings.language == Language::Russian {
            if original.contains("Duplicate key") && original.contains("name_fold:") {
                "Плейлист с таким именем уже существует.".to_owned()
            } else {
                russian.unwrap_or(&original).to_owned()
            }
        } else {
            original
        };
        self.notice = format!(
            "{}: {}",
            self.text("Action failed", "Действие не выполнено"),
            reason.replace('\n', " ")
        );
        self.notice_error = true;
    }
    fn refresh(&mut self) -> Result<()> {
        if !self
            .visible_playlists()
            .iter()
            .any(|playlist| Some(playlist.id) == self.selected_playlist)
        {
            self.selected_playlist = self.visible_playlists().first().map(|playlist| playlist.id);
        }
        if !self
            .rows()
            .iter()
            .any(|entry| Some(entry.id) == self.selected_entry)
        {
            self.selected_entry = self.rows().first().map(|entry| entry.id);
        }
        let valid: HashSet<i64> = self
            .playlist()
            .map(|playlist| playlist.entries.iter().map(|entry| entry.id).collect())
            .unwrap_or_default();
        self.marked.retain(|id| valid.contains(id));
        Ok(())
    }
    fn remember(&mut self, change: Change) -> Result<()> {
        if change.scope != "order" && change.before != change.after {
            let history = self.histories.entry(change.scope).or_default();
            history.1.clear();
            // Results can arrive in a different order from commits. Keep only a connected history.
            if history
                .0
                .last()
                .is_some_and(|previous| previous.after != change.before)
            {
                history.0.clear();
            }
            history.0.push(change);
            if history.0.len() > 100 {
                history.0.remove(0);
            }
        }
        Ok(())
    }
    fn validate_history(&mut self) {
        for (scope, (undo, redo)) in &mut self.histories {
            let expected = undo
                .last()
                .map(|change| &change.after)
                .or_else(|| redo.last().map(|change| &change.before));
            if expected
                .is_some_and(|expected| *expected != Snapshot::from_library(&self.playlists, scope))
            {
                undo.clear();
                redo.clear();
            }
        }
    }
    fn tick_database(&mut self) -> Result<()> {
        let result = self.database.as_mut().map(|job| job.receiver.try_recv());
        let result = match result {
            Some(Ok(result)) => result,
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                Err(anyhow::anyhow!("Background operation was interrupted"))
            }
            _ => return Ok(()),
        };
        let job = self.database.take().expect("database operation is present");
        match result {
            Ok(DatabaseOutcome::Changed(change)) => self.remember(change)?,
            Ok(DatabaseOutcome::Created(change)) => {
                let new = change
                    .after
                    .playlists
                    .iter()
                    .find(|playlist| {
                        !change
                            .before
                            .playlists
                            .iter()
                            .any(|previous| previous.id == playlist.id)
                    })
                    .map(|playlist| playlist.id);
                self.remember(change)?;
                if self.selected_playlist == job.selection && self.settings.mode == job.mode {
                    self.pending_selection = new.map(|id| (id, job.mode));
                }
            }
            Ok(DatabaseOutcome::Renamed) => {}
            Ok(DatabaseOutcome::Restored { change, redo }) => {
                let history = self.histories.entry(change.scope).or_default();
                if redo {
                    history.0.push(change);
                } else {
                    history.1.push(change);
                }
            }
            Err(error) => {
                if let Some((change, redo)) = job.undo {
                    let history = self.histories.entry(change.scope).or_default();
                    if redo {
                        history.1.push(change);
                    } else {
                        history.0.push(change);
                    }
                }
                if let Some(dialog) = job.recovery {
                    self.dialog = Some(dialog);
                }
                if self.import.is_none() {
                    self.validate_history();
                }
                return Err(error);
            }
        }
        self.message(self.text("Library updated", "Библиотека обновлена").into());
        if self.library_revision == self.store.revision() && self.import.is_none() {
            self.validate_history();
        }
        Ok(())
    }
    fn tick_library(&mut self) -> Result<()> {
        // A read can finish before the corresponding command result arrives.
        // Apply its deferred selection even when no additional reload is required.
        if self.library_revision == self.store.revision()
            && let Some((id, mode)) = self.pending_selection.take()
            && self.settings.mode == mode
        {
            self.selected_playlist = Some(id);
            self.focus = Focus::Tracks;
            self.refresh()?;
        }
        let result = self.library.as_mut().map(|job| job.receiver.try_recv());
        match result {
            Some(Ok(result)) => {
                let job = self.library.take().expect("library read is present");
                let playlists = result?;
                if job.revision == self.store.revision() {
                    self.playlists = playlists;
                    self.library_revision = job.revision;
                    if let Some((id, mode)) = self.pending_selection.take()
                        && self.settings.mode == mode
                    {
                        self.selected_playlist = Some(id);
                        self.focus = Focus::Tracks;
                    }
                    self.refresh()?;
                    if self.database.is_none() && self.import.is_none() {
                        self.validate_history();
                    }
                }
            }
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                self.library = None;
                anyhow::bail!("Background operation was interrupted");
            }
            _ => {}
        }
        if self.library.is_none() && self.library_revision != self.store.revision() {
            let store = self.store.try_clone()?;
            self.library = Some(LibraryJob {
                revision: store.revision(),
                receiver: background(self.runtime(), move || store.playlists()),
            });
        }
        Ok(())
    }
    fn tick_settings(&mut self) -> Result<()> {
        let result = self.settings_job.as_mut().map(|job| job.try_recv());
        match result {
            Some(Ok(result)) => {
                self.settings_job = None;
                if let Err(error) = result {
                    self.pending_settings
                        .get_or_insert_with(|| self.settings.clone());
                    self.settings_failed = true;
                    return Err(error);
                }
            }
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                self.settings_job = None;
                self.pending_settings
                    .get_or_insert_with(|| self.settings.clone());
                self.settings_failed = true;
                anyhow::bail!("Background operation was interrupted");
            }
            _ => {}
        }
        self.start_settings()
    }

    pub fn tick(&mut self) {
        if let Err(error) = self.tick_inner() {
            self.failure(error);
        }
    }
    fn tick_inner(&mut self) -> Result<()> {
        self.tick_database()?;
        self.tick_settings()?;
        let outcome = self.import.as_mut().map(|job| job.receiver.try_recv());
        match outcome {
            Some(Ok(outcome)) => {
                let job = self.import.take().expect("import is present");
                let outcome = outcome?;
                let change = outcome.change;
                let mode = change
                    .after
                    .playlists
                    .iter()
                    .find(|playlist| playlist.id == job.target)
                    .map(|playlist| {
                        if playlist.mode == "chaos" {
                            Mode::Chaos
                        } else {
                            Mode::Order
                        }
                    });
                let added = change
                    .after
                    .playlists
                    .iter()
                    .map(|playlist| playlist.entries.len())
                    .sum::<usize>()
                    .saturating_sub(
                        change
                            .before
                            .playlists
                            .iter()
                            .map(|playlist| playlist.entries.len())
                            .sum(),
                    );
                self.remember(change)?;
                if self.pending_selection.is_none()
                    && let Some(mode) = mode
                {
                    self.settings.mode = mode;
                    self.pending_selection = Some((job.target, mode));
                }
                self.save_settings()?;
                let mut message = format!(
                    "{}: {added}. {}: {}",
                    self.text("Added", "Добавлено"),
                    self.text("Skipped", "Пропущено"),
                    outcome.skipped
                );
                if let Some(error) = outcome.errors.first() {
                    message.push_str(&format!(
                        ". {}: {error}",
                        self.text("Import warning", "Ошибка добавления")
                    ));
                }
                self.message(message);
                self.notice_error = !outcome.errors.is_empty();
                if self.library_revision == self.store.revision() && self.database.is_none() {
                    self.validate_history();
                }
            }
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                self.import = None;
                anyhow::bail!("Music import was interrupted");
            }
            _ => {}
        }
        self.tick_library()?;
        if self.audio.as_mut().is_some_and(Audio::finished)
            && let Err(error) = self.next(1, false)
        {
            self.current = None;
            return Err(error);
        }
        Ok(())
    }

    pub fn handle(&mut self, input: Input) {
        if let Err(error) = self.handle_inner(input) {
            self.failure(error);
        }
    }
    fn handle_inner(&mut self, input: Input) -> Result<()> {
        match input {
            Input::Move { x, y } => self.pointer = Position::new(x, y),
            Input::Click { x, y, double } => {
                self.pointer = Position::new(x, y);
                let hit = self
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| hit.enabled && hit.area.contains(Position::new(x, y)))
                    .map(|hit| hit.target.clone());
                if let Some(target) = hit {
                    self.target(target, double)?;
                }
            }
            Input::Scroll { x, y, delta } => {
                if self.dialog.is_some() {
                    self.scroll_dialog(delta);
                } else if self.playlist_area.contains(Position::new(x, y)) {
                    self.focus = Focus::Playlists;
                    self.navigate(delta as i64);
                } else if self.tracks_area.contains(Position::new(x, y)) {
                    self.focus = Focus::Tracks;
                    self.navigate(delta as i64);
                }
            }
            Input::Text(text) => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    let remaining = 512usize.saturating_sub(dialog.text.chars().count());
                    dialog
                        .text
                        .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                }
            }
            Input::Key(key) => self.key(key)?,
        }
        Ok(())
    }

    fn key(&mut self, key: KeyPress) -> Result<()> {
        if key.key == Key::Escape {
            if self.dialog.take().is_none() {
                self.query.clear();
                self.refresh()?;
            }
            return Ok(());
        }
        if let Some(Dialog::CaptureBinding { index }) = self.dialog.as_ref() {
            let index = *index;
            let key = key.normalized();
            ensure!(
                !matches!(
                    key.key,
                    Key::Tab
                        | Key::Up
                        | Key::Down
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown
                ) || key.ctrl
                    || key.alt,
                "Navigation keys are reserved"
            );
            ensure!(
                !self
                    .settings
                    .bindings
                    .iter()
                    .enumerate()
                    .any(|(other, binding)| other != index && binding.key == key),
                "This shortcut is already assigned"
            );
            self.settings.bindings[index].key = key;
            self.save_settings()?;
            self.dialog = Some(Dialog::Settings {
                selected: 6 + index,
            });
            return Ok(());
        }
        if self.dialog.is_some() {
            if matches!(self.dialog, Some(Dialog::Browser(_))) {
                if key.key == Key::Char('a') && key.ctrl {
                    return self.target(Target::BrowserMarkAll, false);
                }
                if (key.key == Key::Enter && key.ctrl) || key.key == Key::Char('a') {
                    return self.browser_add();
                }
                if key.key == Key::Backspace {
                    return self.target(Target::BrowserParent, false);
                }
            }
            if key.ctrl
                && key.key == Key::Char('a')
                && let Some(Dialog::Text(dialog)) = &mut self.dialog
            {
                dialog.selected_all = true;
                return Ok(());
            }
            match key.key {
                Key::Enter => self.submit()?,
                Key::Backspace => {
                    if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                        if dialog.selected_all {
                            dialog.text.clear();
                            dialog.selected_all = false;
                        } else {
                            dialog.text.pop();
                        }
                    }
                }
                Key::Up => self.scroll_dialog(-1),
                Key::Down => self.scroll_dialog(1),
                Key::PageUp => self.scroll_dialog(-8),
                Key::PageDown => self.scroll_dialog(8),
                Key::Char(' ') => {
                    if let Some(Dialog::Browser(browser)) = &mut self.dialog
                        && let Some(entry) = browser.entries.get(browser.selected)
                        && !entry.directory
                        && !browser.marked.insert(entry.path.clone())
                    {
                        browser.marked.remove(&entry.path);
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        let normalized = key.normalized();
        if let Some(action) = self
            .settings
            .bindings
            .iter()
            .find(|binding| binding.key == normalized)
            .map(|binding| binding.action)
        {
            self.action(action)?;
            return Ok(());
        }
        match key.key {
            Key::Tab => {
                self.focus = if self.focus == Focus::Tracks {
                    Focus::Playlists
                } else {
                    Focus::Tracks
                }
            }
            Key::Up => self.navigate(-1),
            Key::Down => self.navigate(1),
            Key::PageUp => self.navigate(-10),
            Key::PageDown => self.navigate(10),
            Key::Home => self.navigate(i64::MIN),
            Key::End => self.navigate(i64::MAX),
            Key::Enter => {
                if self.focus == Focus::Playlists {
                    self.focus = Focus::Tracks;
                } else {
                    self.play_selected()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn navigate(&mut self, direction: i64) {
        if self.focus == Focus::Playlists {
            let playlists = self.visible_playlists();
            let position = playlists
                .iter()
                .position(|playlist| Some(playlist.id) == self.selected_playlist)
                .unwrap_or(0);
            if !playlists.is_empty() {
                let id = playlists[bounded(position, direction, playlists.len())].id;
                self.select_playlist(id);
            }
        } else {
            let rows = self.rows();
            let position = rows
                .iter()
                .position(|entry| Some(entry.id) == self.selected_entry)
                .unwrap_or(0);
            if !rows.is_empty() {
                self.selected_entry = Some(rows[bounded(position, direction, rows.len())].id);
            }
        }
    }
    fn select_playlist(&mut self, id: i64) {
        self.selected_playlist = Some(id);
        self.selected_entry = self
            .playlist()
            .and_then(|playlist| playlist.entries.first())
            .map(|entry| entry.id);
        self.marked.clear();
        self.track_offset = 0;
    }
    fn text_dialog(&mut self, purpose: TextPurpose, text: String) {
        let selected_all = !text.is_empty();
        self.dialog = Some(Dialog::Text(TextDialog {
            purpose,
            text,
            keyboard: self.settings.language,
            upper: false,
            selected_all,
        }));
    }

    pub fn action(&mut self, action: Action) -> Result<()> {
        if !self.allowed(action) {
            self.message(
                self.text(
                    "Action unavailable. Check selection and Order editing.",
                    "Действие недоступно. Проверьте выбор и редактирование Порядка.",
                )
                .into(),
            );
            return Ok(());
        }
        use Action::*;
        match action {
            Quit => self.quit = true,
            Help => self.dialog = Some(Dialog::Help { offset: 0 }),
            Settings => self.dialog = Some(Dialog::Settings { selected: 0 }),
            TogglePlay => {
                if self.current.is_some() {
                    if let Some(audio) = &self.audio {
                        audio.toggle();
                    }
                } else {
                    self.play_selected()?;
                }
            }
            Stop => {
                if let Some(audio) = &mut self.audio {
                    audio.stop();
                }
                self.current = None;
                self.queue.clear();
            }
            Next => self.next(1, true)?,
            Previous => self.next(-1, true)?,
            VolumeUp | VolumeDown => {
                self.settings.volume = (self.settings.volume
                    + if action == VolumeUp { 0.05 } else { -0.05 })
                .clamp(0.0, 1.0);
                if let Some(audio) = &self.audio {
                    audio.volume(self.settings.volume);
                }
                self.save_settings()?;
            }
            SeekForward | SeekBackward => {
                if let Some(audio) = &self.audio {
                    let position = audio.position();
                    audio.seek(if action == SeekForward {
                        position.saturating_add(Duration::from_secs(5))
                    } else {
                        position.saturating_sub(Duration::from_secs(5))
                    })?;
                }
            }
            SwitchMode => self.set_mode(if self.settings.mode == Mode::Order {
                Mode::Chaos
            } else {
                Mode::Order
            })?,
            ToggleEdit => {
                self.editing = !self.editing;
                self.message(
                    self.text("Order editing changed", "Редактирование Порядка изменено")
                        .into(),
                );
            }
            ToggleDesk => {
                self.settings.sorting_desk = !self.settings.sorting_desk;
                self.save_settings()?;
            }
            AddFiles | AddFolder => {
                let directory = dirs::audio_dir()
                    .filter(|directory| directory.is_dir())
                    .or_else(dirs::home_dir)
                    .unwrap_or(std::env::current_dir()?);
                self.dialog = Some(Dialog::Browser(Browser::open(
                    directory,
                    action == AddFolder,
                )?));
            }
            AddNew => {
                let directories: HashSet<PathBuf> = self
                    .playlist()
                    .into_iter()
                    .flat_map(|playlist| &playlist.entries)
                    .filter_map(|entry| entry.track.path.parent().map(Path::to_path_buf))
                    .collect();
                let mut directories: Vec<_> = directories.into_iter().collect();
                directories.sort();
                self.start_import(directories)?;
            }
            NewPlaylist => {
                let mut number = 1;
                let name = loop {
                    let name = format!("{} {number}", self.text("Playlist", "Плейлист"));
                    if !self.playlists.iter().any(|playlist| {
                        playlist.mode == self.settings.mode
                            && playlist.name.to_lowercase() == name.to_lowercase()
                    }) {
                        break name;
                    }
                    number += 1;
                };
                self.text_dialog(TextPurpose::Create, name);
            }
            Rename => {
                if self.focus == Focus::Playlists {
                    if let Some(playlist) = self.playlist() {
                        self.text_dialog(
                            TextPurpose::RenamePlaylist(playlist.id),
                            playlist.name.clone(),
                        );
                    }
                } else if let (Some(playlist), Some(entry)) = (self.playlist(), self.entry()) {
                    self.text_dialog(
                        TextPurpose::RenameTrack(entry.track.id, playlist.id),
                        entry.track.title.clone(),
                    );
                }
            }
            Delete => {
                if self.focus == Focus::Playlists {
                    if let Some(playlist) = self.playlist() {
                        self.text_dialog(
                            TextPurpose::DeletePlaylist(playlist.id, playlist.name.clone()),
                            String::new(),
                        );
                    }
                } else if let (Some(playlist), Some(entry)) = (self.playlist(), self.entry()) {
                    self.dialog = Some(Dialog::RemoveEntry {
                        playlist: playlist.id,
                        entry: entry.id,
                        title: entry.track.title.clone(),
                    });
                }
            }
            MoveUp | MoveDown => {
                let direction = if action == MoveUp { -1 } else { 1 };
                if let Some(playlist) = self.selected_playlist {
                    let focus = self.focus;
                    let entry = self.selected_entry;
                    let unlocked = self.editing;
                    self.start_database(None, move |store| {
                        let change = if focus == Focus::Playlists {
                            store.move_playlist(playlist, direction, unlocked)?
                        } else {
                            store.move_entry(
                                playlist,
                                entry.context("Select a track first")?,
                                direction,
                                unlocked,
                            )?
                        };
                        Ok(DatabaseOutcome::Changed(change))
                    })?;
                }
            }
            Transfer => {
                let ids = self
                    .playlist()
                    .into_iter()
                    .flat_map(|playlist| &playlist.entries)
                    .filter(|entry| {
                        self.marked.contains(&entry.id)
                            || (self.marked.is_empty() && Some(entry.id) == self.selected_entry)
                    })
                    .map(|entry| entry.track.id)
                    .collect();
                self.dialog = Some(Dialog::Transfer { ids, selected: 0 });
            }
            Search => self.text_dialog(TextPurpose::Search, self.query.clone()),
            Sort => {
                self.sort = self.sort.next();
                self.track_offset = 0;
                self.refresh()?;
            }
            Undo | Redo => {
                if let Some(scope) = self.scope() {
                    let change = self.histories.get_mut(scope).and_then(|(undo, redo)| {
                        if action == Undo {
                            undo.pop()
                        } else {
                            redo.pop()
                        }
                    });
                    if let Some(change) = change {
                        let redo = action == Redo;
                        let recovery = change.clone();
                        let result = self.start_database(None, move |store| {
                            let (expected, replacement) = if redo {
                                (&change.before, &change.after)
                            } else {
                                (&change.after, &change.before)
                            };
                            store.restore(scope, expected, replacement)?;
                            Ok(DatabaseOutcome::Restored { change, redo })
                        });
                        match result {
                            Ok(()) => {
                                self.database
                                    .as_mut()
                                    .expect("database request is present")
                                    .undo = Some((recovery, redo))
                            }
                            Err(error) => {
                                let history = self.histories.entry(scope).or_default();
                                if redo {
                                    history.1.push(recovery);
                                } else {
                                    history.0.push(recovery);
                                }
                                return Err(error);
                            }
                        }
                    }
                }
            }
            Metadata => {
                if let Some(entry) = self.entry() {
                    self.dialog = Some(Dialog::Metadata {
                        track: entry.track.clone(),
                        offset: 0,
                    });
                }
            }
            Mark => {
                if let Some(id) = self.selected_entry
                    && !self.marked.insert(id)
                {
                    self.marked.remove(&id);
                }
            }
            PlaylistPanel => {
                self.settings.playlist_placement = self.settings.playlist_placement.next();
                self.save_settings()?;
            }
            PlayerPanel => {
                self.settings.player_placement = self.settings.player_placement.next();
                self.save_settings()?;
            }
        }
        Ok(())
    }

    pub fn set_mode(&mut self, mode: Mode) -> Result<()> {
        if self.busy() {
            return Ok(());
        }
        self.settings.mode = mode;
        self.editing = false;
        self.query.clear();
        self.marked.clear();
        self.selected_playlist = self.visible_playlists().first().map(|playlist| playlist.id);
        self.track_offset = 0;
        self.playlist_offset = 0;
        self.refresh()?;
        self.save_settings()
    }

    fn target(&mut self, target: Target, double: bool) -> Result<()> {
        match target {
            Target::Action(action) => self.action(action)?,
            Target::Mode(mode) => self.set_mode(mode)?,
            Target::Playlist(id) => {
                self.select_playlist(id);
                self.focus = Focus::Playlists;
            }
            Target::Track(id) => {
                self.selected_entry = Some(id);
                self.focus = Focus::Tracks;
                if double {
                    self.play_selected()?;
                }
            }
            Target::Mark(id) => {
                self.selected_entry = Some(id);
                self.focus = Focus::Tracks;
                if !self.marked.insert(id) {
                    self.marked.remove(&id);
                }
            }
            Target::Seek(area) => {
                if let (Some(audio), Some(current)) = (&self.audio, &self.current) {
                    let duration = current.duration_ms;
                    if duration > 0 && area.width > 0 {
                        let point = self.last_pointer_x(area);
                        audio.seek(Duration::from_millis(
                            duration.saturating_mul(point as u64)
                                / area.width.saturating_sub(1).max(1) as u64,
                        ))?;
                    }
                }
            }
            Target::CloseDialog => self.dialog = None,
            Target::Submit => self.submit()?,
            Target::Text(c) => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    if dialog.text.chars().count() < 512 {
                        dialog.text.push(c);
                    }
                }
            }
            Target::Backspace => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    } else {
                        dialog.text.pop();
                    }
                }
            }
            Target::KeyboardLanguage => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    dialog.keyboard = if dialog.keyboard == Language::Russian {
                        Language::English
                    } else {
                        Language::Russian
                    };
                }
            }
            Target::KeyboardCase => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    dialog.upper = !dialog.upper;
                }
            }
            Target::BrowserRow(index) => {
                if let Some(Dialog::Browser(browser)) = &mut self.dialog {
                    browser.selected = index;
                    if let Some(entry) = browser.entries.get(index)
                        && !entry.directory
                        && !browser.marked.insert(entry.path.clone())
                    {
                        browser.marked.remove(&entry.path);
                    }
                }
                if double {
                    self.browser_open()?;
                }
            }
            Target::BrowserParent => {
                if let Some(Dialog::Browser(browser)) = &mut self.dialog
                    && let Some(parent) = browser.directory.parent()
                {
                    browser.directory = parent.to_owned();
                    browser.refresh()?;
                }
            }
            Target::BrowserMarkAll => {
                if let Some(Dialog::Browser(browser)) = &mut self.dialog {
                    for entry in &browser.entries {
                        if !entry.directory {
                            browser.marked.insert(entry.path.clone());
                        }
                    }
                }
            }
            Target::BrowserOpen => self.browser_open()?,
            Target::BrowserAdd => self.browser_add()?,
            Target::TransferRow(index) => {
                if let Some(Dialog::Transfer { selected, .. }) = &mut self.dialog {
                    *selected = index;
                }
                if double {
                    self.submit()?;
                }
            }
            Target::DialogScroll(delta) => self.scroll_dialog(delta),
            Target::Setting(index) => self.setting(index)?,
        }
        Ok(())
    }

    fn last_pointer_x(&self, area: Rect) -> u16 {
        self.pointer.x.saturating_sub(area.x).min(area.width)
    }

    fn browser_open(&mut self) -> Result<()> {
        if let Some(Dialog::Browser(browser)) = &mut self.dialog
            && let Some(entry) = browser.entries.get(browser.selected)
        {
            if entry.directory {
                browser.directory = entry.path.clone();
                browser.refresh()?;
            } else {
                if !browser.marked.insert(entry.path.clone()) {
                    browser.marked.remove(&entry.path);
                }
            }
        }
        Ok(())
    }
    fn browser_add(&mut self) -> Result<()> {
        let paths = if let Some(Dialog::Browser(browser)) = &self.dialog {
            if browser.folder {
                vec![browser.directory.clone()]
            } else if !browser.marked.is_empty() {
                let mut paths: Vec<_> = browser.marked.iter().cloned().collect();
                paths.sort();
                paths
            } else {
                browser
                    .entries
                    .get(browser.selected)
                    .filter(|entry| !entry.directory)
                    .map(|entry| vec![entry.path.clone()])
                    .unwrap_or_default()
            }
        } else {
            return Ok(());
        };
        ensure!(!paths.is_empty(), "Select music files first");
        self.start_import(paths)?;
        self.dialog = None;
        Ok(())
    }

    fn scroll_dialog(&mut self, direction: i16) {
        let transfer_length = self.transfer_destinations().len();
        let settings_length = 6 + self.settings.bindings.len();
        match &mut self.dialog {
            Some(Dialog::Browser(browser)) => {
                browser.selected =
                    bounded(browser.selected, direction as i64, browser.entries.len())
            }
            Some(Dialog::Transfer { selected, .. }) => {
                *selected = bounded(*selected, direction as i64, transfer_length)
            }
            Some(Dialog::Settings { selected }) => {
                *selected = bounded(*selected, direction as i64, settings_length)
            }
            Some(Dialog::Help { offset }) | Some(Dialog::Metadata { offset, .. }) => {
                *offset = bounded(*offset, direction as i64, 1000)
            }
            _ => {}
        }
    }
    pub fn transfer_destinations(&self) -> Vec<&Playlist> {
        self.playlists
            .iter()
            .filter(|playlist| {
                Some(playlist.id) != self.selected_playlist && playlist.can_edit(self.editing)
            })
            .collect()
    }

    fn submit(&mut self) -> Result<()> {
        match self.dialog.take() {
            Some(Dialog::Text(dialog)) => {
                let recovery = Some(Dialog::Text(dialog.clone()));
                let text = dialog.text.clone();
                let mode = self.settings.mode;
                let unlocked = self.editing;
                let result = match &dialog.purpose {
                    TextPurpose::Create => self.start_database(recovery, move |store| {
                        Ok(DatabaseOutcome::Created(
                            store.create_playlist(&text, mode)?,
                        ))
                    }),
                    TextPurpose::RenamePlaylist(id) => {
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.rename_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::RenameTrack(track, playlist) => {
                        let (track, playlist) = (*track, *playlist);
                        self.start_database(recovery, move |store| {
                            store.rename_track(track, &text, playlist, unlocked)?;
                            Ok(DatabaseOutcome::Renamed)
                        })
                    }
                    TextPurpose::DeletePlaylist(id, expected) => {
                        if &dialog.text != expected {
                            self.dialog = Some(Dialog::Text(dialog));
                            anyhow::bail!("Enter the exact playlist name");
                        }
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.delete_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::Search => {
                        self.query = dialog.text.clone();
                        self.track_offset = 0;
                        self.refresh()
                    }
                    TextPurpose::Accent => {
                        let value = dialog.text.trim().trim_start_matches('#');
                        let color = u32::from_str_radix(value, 16)
                            .ok()
                            .filter(|_| value.len() == 6)
                            .context("Use a six-digit color, for example E89E4A");
                        color.and_then(|color| {
                            self.settings.themes[self.settings.mode.index()].accent =
                                [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                            self.save_settings()
                        })
                    }
                };
                if let Err(error) = result {
                    self.dialog = Some(Dialog::Text(dialog));
                    return Err(error);
                }
            }
            Some(Dialog::RemoveEntry {
                playlist,
                entry,
                title,
            }) => {
                let recovery = Dialog::RemoveEntry {
                    playlist,
                    entry,
                    title,
                };
                let unlocked = self.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.remove_entry(playlist, entry, unlocked)?,
                    ))
                }) {
                    self.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::Transfer { ids, selected }) => {
                let destination = self
                    .transfer_destinations()
                    .get(selected)
                    .map(|playlist| playlist.id)
                    .context("No editable destination playlist. Enable Order editing if needed")?;
                let recovery = Dialog::Transfer {
                    ids: ids.clone(),
                    selected,
                };
                let unlocked = self.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(store.copy_tracks(
                        destination,
                        &ids,
                        unlocked,
                    )?))
                }) {
                    self.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::Browser(browser)) => {
                self.dialog = Some(Dialog::Browser(browser));
                self.browser_open()?;
            }
            Some(Dialog::Settings { selected }) => self.setting(selected)?,
            _ => {}
        }
        Ok(())
    }

    fn setting(&mut self, index: usize) -> Result<()> {
        match index {
            0 => {
                self.settings.language = if self.settings.language == Language::Russian {
                    Language::English
                } else {
                    Language::Russian
                }
            }
            1 => {
                let theme = &mut self.settings.themes[self.settings.mode.index()];
                theme.light = !theme.light;
            }
            2 => {
                let accent = self.settings.themes[self.settings.mode.index()].accent;
                self.text_dialog(
                    TextPurpose::Accent,
                    format!("{:02X}{:02X}{:02X}", accent[0], accent[1], accent[2]),
                );
                return Ok(());
            }
            3 => self.settings.playlist_placement = self.settings.playlist_placement.next(),
            4 => self.settings.player_placement = self.settings.player_placement.next(),
            5 => self.settings.bindings = crate::input::default_bindings(),
            _ => {
                if index - 6 < self.settings.bindings.len() {
                    self.dialog = Some(Dialog::CaptureBinding { index: index - 6 });
                }
                return Ok(());
            }
        }
        self.save_settings()?;
        self.dialog = Some(Dialog::Settings { selected: index });
        Ok(())
    }

    pub fn start_import(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        ensure!(self.import.is_none(), "Music import is already running");
        let target = self
            .import_target()
            .context("Select an editable playlist or enable the sorting desk")?;
        let existing: HashSet<PathBuf> = self
            .playlists
            .iter()
            .find(|playlist| playlist.id == target)
            .into_iter()
            .flat_map(|playlist| &playlist.entries)
            .map(|entry| entry.track.path.clone())
            .collect();
        let mut worker_store = self.store.try_clone()?;
        let unlocked = self.editing;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_progress = progress.clone();
        let receiver = background(self.runtime(), move || {
            let mut tracks = Vec::new();
            let mut errors = Vec::new();
            let mut skipped = 0;
            let mut candidates = Vec::new();
            for path in paths {
                if worker_cancel.load(Ordering::Relaxed) {
                    anyhow::bail!("Music import was interrupted");
                }
                if path.is_dir() {
                    match media::audio_files(&path) {
                        Ok(files) => candidates.extend(files),
                        Err(error) => {
                            skipped += 1;
                            if errors.len() < 5 {
                                errors.push(format!("{}: {error}", path.display()));
                            }
                        }
                    }
                } else {
                    candidates.push(path);
                }
            }
            let mut seen = existing;
            for path in candidates {
                if worker_cancel.load(Ordering::Relaxed) {
                    anyhow::bail!("Music import was interrupted");
                }
                if path
                    .canonicalize()
                    .ok()
                    .is_some_and(|path| seen.contains(&path))
                {
                    skipped += 1;
                    continue;
                }
                match media::read_track(&path) {
                    Ok(track) => {
                        if seen.insert(track.path.clone()) {
                            tracks.push(track);
                        } else {
                            skipped += 1;
                        }
                    }
                    Err(error) => {
                        skipped += 1;
                        if errors.len() < 5 {
                            errors.push(error.to_string());
                        }
                    }
                }
                worker_progress.fetch_add(1, Ordering::Relaxed);
            }
            if worker_cancel.load(Ordering::Relaxed) {
                anyhow::bail!("Music import was interrupted");
            }
            let change = worker_store.add_tracks(target, &tracks, unlocked)?;
            Ok(ImportOutcome {
                change,
                errors,
                skipped,
            })
        });
        self.import = Some(ImportJob {
            receiver,
            target,
            cancel,
            progress,
        });
        self.message(
            self.text("Reading music files…", "Чтение музыкальных файлов…")
                .into(),
        );
        Ok(())
    }

    fn play_selected(&mut self) -> Result<()> {
        let entry = self.entry().context("Select a track first")?;
        let queue: Vec<Track> = self
            .playlist()
            .context("Select a playlist first")?
            .entries
            .iter()
            .map(|entry| entry.track.clone())
            .collect();
        let index = queue
            .iter()
            .position(|track| track.id == entry.track.id)
            .context("Track no longer exists")?;
        self.play_track(&queue[index])?;
        self.queue = queue;
        self.queue_index = index;
        Ok(())
    }
    fn play_track(&mut self, track: &Track) -> Result<()> {
        if self.audio.is_none() {
            self.audio = Some(Audio::open(self.settings.volume)?);
        }
        self.audio
            .as_mut()
            .context("Audio output unavailable")?
            .play(&track.path)?;
        self.current = Some(track.clone());
        self.message(format!(
            "{}: {}",
            self.text("Playing", "Воспроизведение"),
            track.title
        ));
        Ok(())
    }
    fn next(&mut self, direction: i64, explicit: bool) -> Result<()> {
        if direction > 0 && self.queue_index + 1 >= self.queue.len() {
            if explicit {
                return Ok(());
            }
            self.current = None;
            self.queue.clear();
            return Ok(());
        }
        if self.queue.is_empty() {
            return Ok(());
        }
        let index = bounded(self.queue_index, direction, self.queue.len());
        let track = self.queue[index].clone();
        self.play_track(&track)?;
        self.queue_index = index;
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(job) = &self.import {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        // Only shutdown waits for settings. Share a deadline with the worker shutdown
        // because started blocking calls cannot be aborted, including settings writes.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let wait = |job: Background<()>| {
            runtime
                .block_on(async {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), job).await
                })
                .context("Settings save did not finish before shutdown")
                .map(|result| {
                    result
                        .context("Settings save was interrupted")
                        .and_then(|result| result)
                })
        };
        let mut result = Ok(());
        let mut expired = false;
        if let Some(job) = self.settings_job.take() {
            match wait(job) {
                Ok(saved) => {
                    if saved.is_err() {
                        self.pending_settings
                            .get_or_insert_with(|| self.settings.clone());
                    }
                    result = saved;
                }
                Err(error) => {
                    expired = true;
                    result = Err(error);
                }
            }
        }
        if !expired && let Some(settings) = self.pending_settings.take() {
            result = self.store.try_clone().and_then(|mut store| {
                let job = background(&runtime, move || store.save_settings(&settings));
                wait(job).and_then(|result| result)
            });
        }
        if let Err(error) = result {
            eprintln!("Almavorn: settings were not saved / настройки не сохранены: {error:#}");
        }
        runtime.shutdown_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
    }
}

fn bounded(position: usize, direction: i64, length: usize) -> usize {
    (position as i128 + direction as i128).clamp(0, length.saturating_sub(1) as i128) as usize
}

pub struct Options {
    pub directory: PathBuf,
    pub files: Vec<PathBuf>,
}

impl Options {
    pub fn parse() -> Result<Option<Self>> {
        let mut directory = dirs::data_local_dir()
            .context("Cannot locate local data directory")?
            .join("almavorn");
        let mut files = Vec::new();
        let mut arguments = std::env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            if argument == "--help" || argument == "-h" {
                println!(
                    "Almavorn\nUsage: almavorn [--data-dir DIRECTORY] [MUSIC FILES...]\nИспользование: almavorn [--data-dir КАТАЛОГ] [МУЗЫКАЛЬНЫЕ ФАЙЛЫ...]\nF1: help / помощь. Q: quit / выход."
                );
                return Ok(None);
            } else if argument == "--data-dir" {
                directory = PathBuf::from(
                    arguments
                        .next()
                        .context("--data-dir requires a directory")?,
                );
            } else if argument.to_string_lossy().starts_with('-') {
                anyhow::bail!("Unknown option: {}", argument.to_string_lossy());
            } else {
                files.push(PathBuf::from(argument));
            }
        }
        Ok(Some(Self { directory, files }))
    }
}
