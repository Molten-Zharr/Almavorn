use super::{
    App, Dialog, FormField, FormPurpose,
    background::{LatestJob, background},
};
use crate::{
    input::{Key, KeyPress},
    lyrics::{self, Lyric, Query, SearchResult},
};
use anyhow::{Context, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
struct Request {
    query: Query,
    force: bool,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
pub struct LyricsState {
    pub query: Option<Query>,
    pub results: Vec<Lyric>,
    pub error: Option<String>,
    pub saved: bool,
    job: LatestJob<Request, SearchResult>,
    cancel: Option<Arc<AtomicBool>>,
}
#[derive(Clone, Default)]
pub struct LyricsView {
    pub selected: usize,
    pub offset: usize,
    pub content: bool,
    pub follow: bool,
    pub confirm_remove: bool,
}
impl LyricsView {
    pub fn content() -> Self {
        Self {
            content: true,
            follow: true,
            ..Self::default()
        }
    }
}
#[derive(Clone, Copy)]
pub enum LyricsAction {
    Row(usize),
    Open,
    Search,
    Refresh,
    Save,
    Edit,
    Export,
    Remove,
    ConfirmRemove,
    Follow,
    Back,
}

impl App {
    pub(super) fn open_lyrics(&mut self) -> Result<()> {
        let track = self
            .playback
            .current
            .as_ref()
            .or_else(|| self.entry().map(|e| &e.track))
            .context("Выберите композицию для поиска текста.")?
            .clone();
        self.start_lyrics(
            Query {
                artist: track.artist.clone(),
                title: track.title.clone(),
                track,
            },
            false,
        );
        Ok(())
    }
    pub(super) fn start_lyrics(&mut self, query: Query, force: bool) {
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.lyrics.query = Some(query.clone());
        self.lyrics.results.clear();
        self.lyrics.error = None;
        self.lyrics.saved = lyrics::cache_path(&self.data_directory(), query.track.id).exists();
        self.lyrics.cancel = Some(cancel.clone());
        self.lyrics.job.request(Request {
            query,
            force,
            cancel,
        });
        self.view.dialog = Some(Dialog::Lyrics(LyricsView::default()));
        self.start_lyrics_job();
    }
    fn start_lyrics_job(&mut self) {
        let data = self.data_directory();
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.lyrics.job.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                if request.cancel.load(Ordering::Relaxed) {
                    return Ok(SearchResult {
                        matches: vec![],
                        warnings: vec![],
                    });
                }
                if !request.force {
                    if let Some(lyric) = lyrics::cached(&data, request.query.track.id)? {
                        return Ok(SearchResult {
                            matches: vec![lyric],
                            warnings: vec![],
                        });
                    }
                    if let Some(lyric) = lyrics::embedded(&request.query.track) {
                        return Ok(SearchResult {
                            matches: vec![lyric],
                            warnings: vec![],
                        });
                    }
                }
                lyrics::search(&request.query, &request.cancel)
            })
        });
    }
    pub fn lyrics_loading(&self) -> bool {
        self.lyrics.job.requested().is_some()
    }
    pub(super) fn tick_lyrics(&mut self) {
        let Some(completed) = self.lyrics.job.poll() else {
            return;
        };
        self.start_lyrics_job();
        if !completed.current {
            return;
        }
        match completed.result {
            Ok(result) => {
                self.lyrics.results = result.matches;
                self.lyrics.error =
                    (!result.warnings.is_empty()).then(|| result.warnings.join("\n"));
                self.lyrics.saved =
                    lyrics::cache_path(&self.data_directory(), completed.request.query.track.id)
                        .exists();
                if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
                    view.selected = 0;
                    view.offset = 0;
                    if self.lyrics.results.len() == 1 && self.lyrics.results[0].score >= 900
                        || self.lyrics.results.first().is_some_and(|first| {
                            first.score >= 980
                                && self.lyrics.results.get(1).is_none_or(|second| {
                                    first.score.saturating_sub(second.score) >= 50
                                })
                        })
                    {
                        view.content = true;
                        view.follow = true;
                    }
                }
            }
            Err(error) => self.lyrics.error = Some(format!("{error:#}")),
        }
    }
    pub(super) fn lyrics_close(&mut self) -> bool {
        let Some(Dialog::Lyrics(view)) = &mut self.view.dialog else {
            return false;
        };
        if view.confirm_remove {
            view.confirm_remove = false;
            return true;
        }
        if view.content && self.lyrics.results.len() > 1 {
            view.content = false;
            view.offset = 0;
            return true;
        }
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.lyrics.job.cancel();
        self.view.dialog = None;
        true
    }
    pub(super) fn lyrics_scroll(&mut self, direction: i64) {
        if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
            if view.content {
                view.follow = false;
                view.offset =
                    super::bounded(view.offset, direction, self.view.dialog_scroll_max + 1);
            } else {
                view.selected = super::bounded(view.selected, direction, self.lyrics.results.len());
            }
        }
    }
    pub(super) fn lyrics_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Lyrics(view)) = &self.view.dialog else {
            return Ok(());
        };
        if view.confirm_remove {
            return match key.key {
                Key::Char('d') => self.lyrics_action(LyricsAction::ConfirmRemove),
                Key::Enter => self.lyrics_action(LyricsAction::Back),
                _ => Ok(()),
            };
        }
        match key.key {
            Key::Enter => self.lyrics_action(LyricsAction::Open)?,
            Key::Up => self.lyrics_scroll(-1),
            Key::Down => self.lyrics_scroll(1),
            Key::PageUp => self.lyrics_scroll(-10),
            Key::PageDown => self.lyrics_scroll(10),
            Key::Char('s') => self.lyrics_action(LyricsAction::Save)?,
            Key::Char('e') => self.lyrics_action(LyricsAction::Edit)?,
            Key::Char('f') => self.lyrics_action(LyricsAction::Search)?,
            Key::Char('r') => self.lyrics_action(LyricsAction::Refresh)?,
            Key::Char('l') => self.lyrics_action(LyricsAction::Follow)?,
            Key::Char('x') => self.lyrics_action(LyricsAction::Export)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn lyrics_action(&mut self, action: LyricsAction) -> Result<()> {
        let result = self.lyrics_action_inner(action);
        if let Err(error) = result {
            self.lyrics.error = Some(format!("{error:#}"));
        }
        Ok(())
    }
    fn lyrics_action_inner(&mut self, action: LyricsAction) -> Result<()> {
        if matches!(action, LyricsAction::Back) {
            self.lyrics_close();
            return Ok(());
        }
        let Some(Dialog::Lyrics(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        if let LyricsAction::Row(index) = action {
            view.selected = index.min(self.lyrics.results.len().saturating_sub(1));
            return Ok(());
        }
        let query = self.lyrics.query.clone().context("Выберите композицию.")?;
        let selected = view.selected;
        match action {
            LyricsAction::Search => {
                let title = self.text("Find lyrics", "Найти текст песни");
                let description=self.text("Enter the artist and title. If no close match is found, the player also searches by title words.", "Укажите исполнителя и название. Если точного совпадения нет, плеер ищет также по словам названия.");
                let fields = vec![
                    FormField::new(self.text("Artist", "Исполнитель"), query.artist),
                    FormField::new(self.text("Title", "Название"), query.title),
                ];
                self.open_form(
                    title,
                    description,
                    fields,
                    FormPurpose::LyricsQuery(query.track),
                );
            }
            LyricsAction::Refresh => self.start_lyrics(query, true),
            LyricsAction::Open => {
                if selected < self.lyrics.results.len() {
                    view.content = true;
                    view.follow = true;
                    view.offset = 0;
                }
            }
            LyricsAction::Follow => view.follow = !view.follow,
            LyricsAction::Remove => view.confirm_remove = true,
            LyricsAction::ConfirmRemove => {
                lyrics::remove(&self.data_directory(), query.track.id)?;
                self.lyrics.saved = false;
                if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
                    view.confirm_remove = false;
                }
            }
            LyricsAction::Save => {
                let lyric = self
                    .lyrics
                    .results
                    .get(selected)
                    .context("Выберите найденный текст.")?;
                lyrics::save(&self.data_directory(), query.track.id, lyric)?;
                self.lyrics.saved = true;
                self.lyrics.error = None;
            }
            LyricsAction::Edit => {
                let lyric = self
                    .lyrics
                    .results
                    .get(selected)
                    .context("Выберите найденный текст.")?
                    .clone();
                let mut field = FormField::new(self.text("Lyrics", "Текст песни"), lyric.content());
                field.multiline = true;
                self.open_form(
                    self.text("Edit saved lyrics", "Изменить сохраненный текст"),
                    self.text(
                        "Ctrl+S: save. These changes are kept in Almavorn.",
                        "Ctrl+S: сохранить. Изменения сохраняются в Almavorn.",
                    ),
                    vec![field],
                    FormPurpose::LyricsEdit(query.track.id, lyric),
                );
            }
            LyricsAction::Export => {
                let lyric = self
                    .lyrics
                    .results
                    .get(selected)
                    .context("Выберите найденный текст.")?
                    .clone();
                let extension = if lyric.synced.is_empty() {
                    "txt"
                } else {
                    "lrc"
                };
                let path = query.track.path.with_extension(extension);
                self.open_form(
                    self.text("Save lyrics to file", "Сохранить текст в файл"),
                    self.text(
                        "Choose a full file path. Existing files will be kept.",
                        "Укажите полный путь к новому файлу.",
                    ),
                    vec![FormField::new(
                        self.text("File path", "Путь к файлу"),
                        path.display().to_string(),
                    )],
                    FormPurpose::LyricsExport(lyric),
                );
            }
            _ => {}
        }
        Ok(())
    }
}

impl Drop for LyricsState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
