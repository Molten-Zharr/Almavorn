use super::{
    App, Dialog, FormField, FormPurpose,
    background::{Background, LatestJob, background, poll},
};
use crate::{
    input::{Key, KeyPress},
    lyrics::{self, Lyric, Provider, ProviderReport, Query, SearchResult},
};
use anyhow::{Context, Result};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
mod panel;
pub use panel::{LyricsPanel, PanelAction};

#[derive(Clone)]
struct Request {
    query: Query,
    force: bool,
    cancel: Arc<AtomicBool>,
    progress: mpsc::Sender<ProviderReport>,
    preferences: crate::preferences::LyricsPreferences,
    excluded: Vec<Provider>,
}
#[derive(Default)]
pub struct LyricsState {
    pub query: Option<Query>,
    pub results: Vec<Lyric>,
    pub error: Option<String>,
    pub saved: bool,
    pub reports: Vec<ProviderReport>,
    pub excluded: Vec<Provider>,
    pub panel: LyricsPanel,
    pub network_paused: bool,
    last_preferences: Option<crate::preferences::LyricsPreferences>,
    progress: Option<mpsc::Receiver<ProviderReport>>,
    key_job: Option<Background<()>>,
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
    pub sources: bool,
    pub confirm_key: Option<Provider>,
    pub panel_result: bool,
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
    Sources,
    Key(Provider),
    KeyPage(Provider),
    RemoveKey(Provider),
    ConfirmKey,
    Services,
    FindOther,
    Settings,
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
        if self
            .lyrics
            .panel
            .query
            .as_ref()
            .is_some_and(|q| q.track.id == track.id)
        {
            return self.open_panel_lyrics();
        }
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
        self.start_lyrics_excluding(query, force, vec![]);
    }
    fn start_lyrics_excluding(&mut self, query: Query, force: bool, excluded: Vec<Provider>) {
        if self.settings.preferences.lyrics.enabled {
            self.resume_lyrics_network();
        }
        if self
            .lyrics
            .panel
            .query
            .as_ref()
            .is_some_and(|q| q.track.id == query.track.id)
        {
            self.lyrics.panel.cancel();
        }
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.lyrics.query = Some(query.clone());
        self.lyrics.results.clear();
        self.lyrics.reports.clear();
        self.lyrics.excluded = excluded.clone();
        let (progress, receiver) = mpsc::channel();
        self.lyrics.progress = Some(receiver);
        self.lyrics.error = None;
        self.lyrics.saved = lyrics::cache_path(&self.data_directory(), query.track.id).exists();
        self.lyrics.cancel = Some(cancel.clone());
        self.lyrics.job.request(Request {
            query,
            force,
            cancel,
            progress,
            preferences: self.settings.preferences.lyrics.clone(),
            excluded,
        });
        self.view.dialog = Some(Dialog::Lyrics(LyricsView::default()));
        self.start_lyrics_job();
    }
    fn start_lyrics_job(&mut self) {
        let data = self.data_directory();
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.lyrics.job.start(|request| {
            let request = request.clone();
            background(runtime, move || run_request(&data, &request))
        });
    }
    pub fn lyrics_loading(&self) -> bool {
        self.lyrics.job.requested().is_some()
            || matches!(&self.view.dialog, Some(Dialog::Lyrics(view)) if view.panel_result)
                && self.lyrics.panel_loading()
    }
    pub(super) fn tick_lyrics(&mut self) {
        self.sync_lyrics_preferences();
        self.tick_lyrics_panel();
        if matches!(&self.view.dialog, Some(Dialog::Lyrics(view)) if view.panel_result) {
            self.lyrics.query = self.lyrics.panel.query.clone();
            self.lyrics.excluded = self.lyrics.panel.excluded.clone();
            self.lyrics.reports = self.lyrics.panel.reports.clone();
            if !self.lyrics.panel_loading() {
                self.lyrics.results = self.lyrics.panel.results.clone();
                self.lyrics.error = self.lyrics.panel.error.clone();
                if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
                    view.panel_result = false;
                    view.content = self.lyrics.panel.lyric().is_some();
                }
            }
        }
        if let Some(receiver) = &self.lyrics.progress {
            for report in receiver.try_iter() {
                self.lyrics
                    .reports
                    .retain(|r| r.provider != report.provider);
                self.lyrics.reports.push(report);
            }
        }
        if let Some(result) = self.lyrics.key_job.as_mut().and_then(poll) {
            self.lyrics.key_job = None;
            match result {
                Ok(()) => {
                    if matches!(self.view.dialog, Some(Dialog::Lyrics(_)))
                        && let Some(query) = self.lyrics.query.clone()
                    {
                        self.start_lyrics(query, true);
                        if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
                            view.sources = true;
                        }
                    }
                }
                Err(error) => self.lyrics.error = Some(error.to_string()),
            }
        }
        let Some(completed) = self.lyrics.job.poll() else {
            return;
        };
        self.start_lyrics_job();
        if !completed.current {
            return;
        }
        if let Some(receiver) = &self.lyrics.progress {
            for report in receiver.try_iter() {
                self.lyrics
                    .reports
                    .retain(|r| r.provider != report.provider);
                self.lyrics.reports.push(report);
            }
        }
        match completed.result {
            Ok(result) => {
                if result.network_failed {
                    self.lyrics.network_paused = true;
                }
                self.lyrics.results = result.matches;
                self.lyrics.error = (!result.warnings.is_empty()).then(|| {
                    self.text(
                        "Some sources could not complete the search. See Sources for details.",
                        "Часть источников не завершила поиск. Подробности: Источники.",
                    )
                    .into()
                });
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
                if matches!(&self.view.dialog, Some(Dialog::Lyrics(view)) if view.content) {
                    self.update_panel_from_dialog();
                }
            }
            Err(error) => self.lyrics.error = Some(format!("{error:#}")),
        }
        if self
            .lyrics
            .query
            .as_ref()
            .zip(self.lyrics.panel.query.as_ref())
            .is_some_and(|(manual, panel)| manual.track.id == panel.track.id)
        {
            self.lyrics.panel.cancel();
            self.lyrics.panel.query = self.lyrics.query.clone();
            self.lyrics.panel.results = self.lyrics.results.clone();
            self.lyrics.panel.reports = self.lyrics.reports.clone();
            self.lyrics.panel.excluded = self.lyrics.excluded.clone();
            self.lyrics.panel.error = self.lyrics.error.clone();
            self.lyrics.panel.remember();
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
        if view.confirm_key.take().is_some() {
            return true;
        }
        if view.sources {
            view.sources = false;
            view.offset = 0;
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
            if view.content || view.sources {
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
        if view.confirm_key.is_some() {
            return match key.key {
                Key::Char('d') => self.lyrics_action(LyricsAction::ConfirmKey),
                Key::Enter => self.lyrics_action(LyricsAction::Back),
                _ => Ok(()),
            };
        }
        if key.key == Key::Char('i') {
            return self.lyrics_action(LyricsAction::Sources);
        }
        if view.sources {
            return match key.key {
                Key::Char('r') => self.lyrics_action(LyricsAction::Refresh),
                Key::Up => {
                    self.lyrics_scroll(-1);
                    Ok(())
                }
                Key::Down => {
                    self.lyrics_scroll(1);
                    Ok(())
                }
                Key::PageUp => {
                    self.lyrics_scroll(-(self.view.dialog_page_size as i64));
                    Ok(())
                }
                Key::PageDown => {
                    self.lyrics_scroll(self.view.dialog_page_size as i64);
                    Ok(())
                }
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
            Key::Char('n') => self.lyrics_action(LyricsAction::FindOther)?,
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
            LyricsAction::Sources => {
                view.sources = !view.sources;
                view.offset = 0;
            }
            LyricsAction::Key(provider) => {
                anyhow::ensure!(self.lyrics.key_job.is_none(), "Дождитесь сохранения ключа.");
                let mut field = FormField::new(self.text("New key", "Новый ключ"), "");
                field.secret = true;
                self.open_form(
                    provider.name(),
                    if provider == Provider::Genius {
                        self.text("Optional Client Access Token from genius.com/api-clients. Without it the player uses public search.", "Необязательный Client Access Token с genius.com/api-clients. Без него работает публичный поиск.")
                    } else {
                        self.text("Enter your Musixmatch API key. Text availability depends on your API access.", "Введите ключ API Musixmatch. Доступ к текстам зависит от вашего тарифа.")
                    },
                    vec![field], FormPurpose::LyricsKey(provider),
                );
            }
            LyricsAction::KeyPage(provider) => {
                let mut child = std::process::Command::new("xdg-open")
                    .arg(provider.key_url())
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .context("Не удалось открыть браузер.")?;
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            LyricsAction::RemoveKey(provider) => {
                view.confirm_key = Some(provider);
            }
            LyricsAction::ConfirmKey => {
                if let Some(provider) = view.confirm_key {
                    self.lyrics_set_key(provider, None)?;
                }
            }
            LyricsAction::Services => self.open_services(),
            LyricsAction::Settings => self.open_settings_page(super::SettingsPage::Lyrics),
            LyricsAction::FindOther => {
                let mut excluded = self.lyrics.excluded.clone();
                if let Some(lyric) = self.lyrics.results.get(selected)
                    && let Some(provider) = Provider::ALL
                        .into_iter()
                        .find(|p| p.name() == lyric.provider)
                    && !excluded.contains(&provider)
                {
                    excluded.push(provider);
                }
                self.start_lyrics_excluding(query, true, excluded);
            }
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
            LyricsAction::Refresh => {
                let sources = view.sources;
                self.start_lyrics_excluding(query, true, self.lyrics.excluded.clone());
                if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog {
                    view.sources = sources;
                }
            }
            LyricsAction::Open => {
                if selected < self.lyrics.results.len() {
                    view.content = true;
                    view.follow = true;
                    view.offset = 0;
                }
                self.update_panel_from_dialog();
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
                self.update_panel_from_dialog();
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
    pub fn lyrics_key_busy(&self) -> bool {
        self.lyrics.key_job.is_some()
    }
    pub(super) fn lyrics_set_key(&mut self, provider: Provider, key: Option<String>) -> Result<()> {
        anyhow::ensure!(self.lyrics.key_job.is_none(), "Дождитесь сохранения ключа.");
        if let Some(key) = &key {
            anyhow::ensure!(!key.trim().is_empty(), "Введите ключ.");
        }
        let data = self.data_directory();
        self.lyrics.error = None;
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.lyrics.job.cancel();
        self.lyrics.progress = None;
        self.lyrics.key_job = Some(background(
            self.runtime.as_ref().expect("runtime is alive"),
            move || lyrics::set_key(&data, provider, key.as_deref().map(str::trim)),
        ));
        self.view.dialog = Some(Dialog::Lyrics(LyricsView {
            sources: true,
            ..LyricsView::default()
        }));
        Ok(())
    }
}

fn run_request(data: &std::path::Path, request: &Request) -> Result<SearchResult> {
    if request.cancel.load(Ordering::Relaxed) {
        return Ok(SearchResult::default());
    }
    let mut warnings = vec![];
    if !request.force || !request.preferences.enabled {
        match lyrics::cached(data, request.query.track.id) {
            Ok(Some(mut lyric)) => {
                lyric.score = 1000;
                return Ok(SearchResult {
                    matches: vec![lyric],
                    ..SearchResult::default()
                });
            }
            Err(error) => warnings.push(error.to_string()),
            _ => {}
        }
        if request.query.track.path.is_file()
            && let Some(lyric) = lyrics::embedded(&request.query.track)
        {
            return Ok(SearchResult {
                matches: vec![lyric],
                ..SearchResult::default()
            });
        }
    }
    if !request.preferences.enabled {
        return Ok(SearchResult {
            warnings,
            ..SearchResult::default()
        });
    }
    let mut result = lyrics::search(
        data,
        &request.query,
        &request.cancel,
        &request.progress,
        &request.preferences,
        &request.excluded,
    )?;
    result.warnings.extend(warnings);
    Ok(result)
}

impl Drop for LyricsState {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
