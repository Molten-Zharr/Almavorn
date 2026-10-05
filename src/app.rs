mod actions;
mod browser;
mod database;
mod dialogs;
mod events;
mod history;
mod import;
mod library;
mod options;
mod playback;
mod settings;
mod state;

pub use browser::{Browser, BrowserEntry};
pub use dialogs::{Dialog, TextDialog, TextPurpose};
pub use options::Options;
pub use settings::{SettingsCatalog, SettingsEdit, SettingsFocus, SettingsPage};
pub use state::{Focus, Hit, Sort, Target};

use self::{
    database::{Background, DatabaseJob, LibraryJob, background},
    import::ImportJob,
};
use crate::{
    audio::Audio,
    model::{Language, Mode, Playlist, Settings, Track},
    store::{Change, Store},
};
use anyhow::{Context, Result};
use ratatui::layout::{Position, Rect};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::runtime::{Builder, Runtime};

pub struct App {
    pub store: Store,
    runtime: Option<Runtime>,
    pub settings: Settings,
    pub(crate) settings_view: settings::SettingsView,
    pub(crate) settings_file: Option<settings::SettingsFileJob>,
    browser_job: Option<browser::BrowserJob>,
    pending_browser: Option<browser::BrowserRequest>,
    browser_generation: u64,
    browser_ready: bool,
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
    queue: Arc<[Track]>,
    queue_index: usize,
    audio: Option<Audio>,
    playback_job: Option<playback::PlaybackJob>,
    pending_playback: Option<playback::PlaybackRequest>,
    playback_generation: u64,
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
            settings_view: settings::SettingsView::default(),
            settings_file: None,
            browser_job: None,
            pending_browser: None,
            browser_generation: 0,
            browser_ready: false,
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
            queue: Arc::from([]),
            queue_index: 0,
            audio: None,
            playback_job: None,
            pending_playback: None,
            playback_generation: 0,
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
            "Choose an available folder before adding music" => {
                Some("Выберите доступную папку и дождитесь ее загрузки.")
            }
            "Music import is already running" => {
                Some("Добавление музыкальных файлов уже выполняется.")
            }
            "This shortcut is already assigned" => Some("Это сочетание клавиш уже назначено."),
            "Navigation keys are reserved" => Some("Эти клавиши зарезервированы для навигации."),
            "Use a six-digit color, for example E89E4A" => {
                Some("Введите шесть цифр цвета, например E89E4A.")
            }
            "Use a six-digit color, for example FF0000" => {
                Some("Введите цвет из шести шестнадцатеричных цифр, например FF0000.")
            }
            "Font size must be 10-32 pixels" => {
                Some("Размер шрифта должен быть от 10 до 32 пикселей.")
            }
            "Name must contain 1-80 printable characters" => {
                Some("Имя должно содержать от 1 до 80 печатных символов.")
            }
            "This name is already used" => Some("Это имя уже используется. Выберите другое."),
            "Playlist with this name already exists" => {
                Some("Плейлист с таким именем уже существует.")
            }
            "Database writer failed" => {
                Some("Не удалось выполнить запись в библиотеку. Перезапустите плеер.")
            }
            "Wait for the palette file operation to finish" => {
                Some("Дождитесь завершения импорта или экспорта палитры.")
            }
            "Enter a file path" => Some("Введите путь к файлу."),
            "Cannot read palette JSON" => {
                Some("Не удалось прочитать JSON палитры. Проверьте файл.")
            }
            "Unsupported palette format or version" => {
                Some("Формат или версия палитры не поддерживаются.")
            }
            "Expected an Almavorn or Molten-Zharr palette" => {
                Some("Ожидается палитра Almavorn или Molten-Zharr.")
            }
            "Palette file is larger than 1 MiB" => {
                Some("Файл палитры должен быть не больше 1 МиБ.")
            }
            "Keep at least one item in this catalog" => {
                Some("В этом списке должен остаться хотя бы один элемент.")
            }
            "Keep at least one profile" => Some("Должен остаться хотя бы один профиль."),
            "Keep at least one palette" => Some("Должна остаться хотя бы одна палитра."),
            "Keep at least one theme preset" => Some("Должен остаться хотя бы один пресет темы."),
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
            "Unsupported or damaged audio file" => {
                Some("Аудиофайл поврежден или его формат не поддерживается.")
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

    pub fn tick(&mut self) {
        if let Err(error) = self.tick_inner() {
            self.failure(error);
        }
    }

    fn tick_inner(&mut self) -> Result<()> {
        self.tick_database()?;
        self.tick_settings()?;
        self.tick_settings_files()?;
        self.tick_browser()?;
        self.tick_playback()?;
        self.poll_import()?;
        self.tick_library()?;
        if !self.preparing_playback()
            && self.audio.as_mut().is_some_and(Audio::finished)
            && let Err(error) = self.next(1, false)
        {
            self.current = None;
            return Err(error);
        }
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
