use super::{
    App, Dialog, Lyric, LyricsState, LyricsView, Provider, ProviderReport, Query, Request,
    SearchResult, background, run_request,
};
use crate::{
    app::background::{Background, LatestJob, poll},
    input::{Action, Key, KeyPress},
    model::Track,
};
use anyhow::{Context, Result};
use ratatui::layout::Rect;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq)]
struct TrackKey {
    id: i64,
    path: PathBuf,
    artist: String,
    title: String,
    duration: u64,
}
impl From<&Track> for TrackKey {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id,
            path: track.path.clone(),
            artist: track.artist.clone(),
            title: track.title.clone(),
            duration: track.duration_ms,
        }
    }
}
struct Cached {
    key: TrackKey,
    results: Vec<Lyric>,
    reports: Vec<ProviderReport>,
    excluded: Vec<Provider>,
    selected: Option<usize>,
    saved: bool,
}

#[derive(Default)]
pub struct LyricsPanel {
    pub saved: bool,
    pub query: Option<Query>,
    pub results: Vec<Lyric>,
    pub reports: Vec<ProviderReport>,
    pub excluded: Vec<Provider>,
    pub error: Option<String>,
    pub area: Rect,
    pub offset: usize,
    pub scroll_max: usize,
    pub page_size: usize,
    pub follow: bool,
    pub selected: Option<usize>,
    pub choosing: bool,
    pub cursor: usize,
    pub focused: bool,
    pub buttons: Vec<PanelAction>,
    pub button: Option<PanelAction>,
    key: Option<TrackKey>,
    due: Option<Instant>,
    cache: VecDeque<Cached>,
    local_job: Option<Background<Option<Lyric>>>,
    job: LatestJob<Request, SearchResult>,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<mpsc::Receiver<ProviderReport>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PanelAction {
    Choose(usize),
    Variants,
    Content,
    Focus,
    Open,
    Other,
    Retry,
    Settings,
    Sources,
    Save,
    Remove,
    Close,
}

impl PanelAction {
    pub(crate) fn binding(self) -> Option<Action> {
        Some(match self {
            Self::Open => Action::Lyrics,
            Self::Save => Action::LyricsSave,
            Self::Remove => Action::LyricsRemove,
            Self::Retry => Action::LyricsRetry,
            Self::Other => Action::LyricsOther,
            Self::Variants | Self::Content => Action::LyricsVariants,
            Self::Sources => Action::LyricsSources,
            Self::Settings => Action::LyricsSettings,
            Self::Close => Action::ToggleLyricsPanel,
            Self::Choose(_) | Self::Focus => return None,
        })
    }

    pub(crate) fn local_key(self) -> Option<&'static str> {
        match self {
            Self::Save => Some("S"),
            Self::Remove => Some("Del"),
            Self::Retry => Some("R"),
            Self::Other => Some("N"),
            Self::Variants | Self::Content => Some("V"),
            Self::Sources => Some("I"),
            Self::Settings => Some("K"),
            Self::Choose(_) => Some("Enter"),
            _ => None,
        }
    }
}

impl LyricsPanel {
    pub(super) fn cancel(&mut self) {
        self.local_job = None;
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.job.cancel();
        self.progress = None;
        self.due = None;
    }
    pub(super) fn remember(&mut self) {
        let Some(key) = self.key.clone() else {
            return;
        };
        self.cache.retain(|entry| entry.key != key);
        let mut indices: Vec<_> = (0..self.results.len().min(3)).collect();
        if let Some(selected) = self.selected.filter(|index| *index < self.results.len())
            && !indices.contains(&selected)
        {
            indices.pop();
            indices.push(selected);
        }
        self.cache.push_back(Cached {
            key,
            saved: self.saved,
            selected: self
                .selected
                .and_then(|selected| indices.iter().position(|index| *index == selected)),
            results: indices
                .into_iter()
                .map(|index| self.results[index].clone())
                .collect(),
            reports: self.reports.clone(),
            excluded: self.excluded.clone(),
        });
        while self.cache.len() > 24 {
            self.cache.pop_front();
        }
    }
    pub fn lyric(&self) -> Option<&Lyric> {
        if self.choosing {
            None
        } else {
            self.selected.and_then(|index| self.results.get(index))
        }
    }
    pub(super) fn saved_changed(&mut self, track: i64, lyric: Option<Lyric>) {
        let saved = lyric.is_some();
        for entry in self.cache.iter_mut().filter(|entry| entry.key.id == track) {
            entry.saved = saved;
            if let Some(lyric) = &lyric { entry.results = vec![lyric.clone()]; entry.selected = Some(0); }
        }
        if self.query.as_ref().is_some_and(|query| query.track.id == track) {
            self.cancel();
            self.saved = saved;
            if let Some(lyric) = lyric {
                self.results = vec![lyric]; self.selected = Some(0); self.cursor = 0; self.choosing = false;
                self.offset = 0; self.follow = true;
            }
            self.remember();
        }
    }
}
impl Drop for LyricsPanel {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl LyricsState {
    pub fn panel_loading(&self) -> bool {
        self.panel.local_job.is_some() || self.panel.job.requested().is_some() || self.panel.due.is_some()
    }
}

impl App {
    pub(super) fn resume_lyrics_network(&mut self) {
        if self.lyrics.network_paused {
            self.lyrics
                .panel
                .cache
                .retain(|entry| !entry.results.is_empty());
        }
        self.lyrics.network_paused = false;
    }
    pub(super) fn sync_lyrics_preferences(&mut self) {
        let preferences = self.settings.preferences.lyrics.clone();
        if self.lyrics.last_preferences.as_ref() == Some(&preferences) {
            return;
        }
        if self.lyrics.last_preferences.is_none() {
            self.lyrics.last_preferences = Some(preferences);
            return;
        }
        if let Some(old) = &self.lyrics.last_preferences {
            if !old.enabled && preferences.enabled {
                self.lyrics.network_paused = false;
                self.lyrics
                    .panel
                    .cache
                    .retain(|entry| !entry.results.is_empty());
            }
            if old.disabled != preferences.disabled {
                self.lyrics.panel.cache.clear();
            }
        }
        self.lyrics.last_preferences = Some(preferences);
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.lyrics.job.cancel();
        self.lyrics.progress = None;
        self.lyrics.panel.cancel();
        self.lyrics.panel.key = None;
    }

    pub(super) fn tick_lyrics_panel(&mut self) {
        if let Some(progress) = &self.lyrics.panel.progress {
            self.lyrics.panel.reports.extend(progress.try_iter());
        }
        if let Some(completed) = self.lyrics.panel.job.poll()
            && completed.current
        {
            if let Some(progress) = &self.lyrics.panel.progress {
                self.lyrics.panel.reports.extend(progress.try_iter());
            }
            match completed.result {
                Ok(result) => {
                    self.lyrics.panel.saved = result.saved;
                    if result.network_failed {
                        self.lyrics.network_paused = true;
                    }
                    self.lyrics.panel.results = result.matches;
                    self.lyrics.panel.selected =
                        crate::lyrics::automatic_match(&self.lyrics.panel.results);
                    self.lyrics.panel.cursor = self.lyrics.panel.selected.unwrap_or(0);
                    self.lyrics.panel.choosing = self.lyrics.panel.selected.is_none();
                    self.lyrics.panel.error = if self.lyrics.panel.results.is_empty()
                        && !result.warnings.is_empty()
                    {
                        Some(self.text("Some sources could not complete the search. See the source results below.", "Часть источников не завершила поиск. Результаты по источникам показаны ниже.").into())
                    } else {
                        None
                    };
                    self.lyrics.panel.remember();
                }
                Err(error) => {
                    self.lyrics.panel.error = Some(error.to_string());
                    self.lyrics.panel.remember();
                }
            }
        }
        if !self.settings.preferences.lyrics.show_panel {
            return;
        }
        let track = self.playback.current.clone();
        let key = track.as_ref().map(TrackKey::from);
        if key != self.lyrics.panel.key || track.is_none() && self.lyrics.panel.query.is_some() {
            let panel = &mut self.lyrics.panel;
            panel.cancel();
            panel.key = key.clone();
            panel.results.clear();
            panel.saved = false;
            panel.selected = None;
            panel.choosing = false;
            panel.cursor = 0;
            panel.button = None;
            panel.reports.clear();
            panel.excluded.clear();
            panel.error = None;
            panel.offset = 0;
            panel.follow = true;
            panel.query = track.map(|track| Query {
                artist: track.artist.clone(),
                title: track.title.clone(),
                track,
            });
            if let Some(key) = key {
                // A saved copy takes precedence over any later, unsaved search
                // result in the memory cache when this track starts again.
                if let Some(cached) = panel.cache.iter().find(|entry| entry.key == key && !entry.saved) {
                    panel.results = cached.results.clone();
                    panel.saved = cached.saved;
                    panel.selected = cached.selected;
                    panel.cursor = cached.selected.unwrap_or(0);
                    panel.choosing = cached.selected.is_none();
                    panel.reports = cached.reports.clone();
                    panel.excluded = cached.excluded.clone();
                } else {
                    panel.due = Some(Instant::now() + Duration::from_millis(450));
                    // Local lyrics must not wait for the network debounce or an
                    // older provider request that is still finishing.
                    let store = self.store.try_clone();
                    panel.local_job = Some(background(self.runtime.as_ref().expect("runtime is alive"), move || {
                        store?.saved_lyrics(key.id)
                    }));
                }
            }
        }
        if let Some(result) = self.lyrics.panel.local_job.as_mut().and_then(poll) {
            self.lyrics.panel.local_job = None;
            match result {
                Ok(Some(mut lyric)) => {
                    lyric.score = 1000;
                    let panel = &mut self.lyrics.panel;
                    panel.cancel();
                    panel.results = vec![lyric];
                    panel.saved = true;
                    panel.selected = Some(0);
                    panel.cursor = 0;
                    panel.choosing = false;
                    panel.error = None;
                    panel.remember();
                }
                Ok(None) => {}
                Err(error) => {
                    self.lyrics.panel.due = None;
                    self.lyrics.panel.error = Some(error.to_string());
                }
            }
        }
        if self
            .lyrics
            .panel
            .due
            .is_some_and(|due| Instant::now() >= due)
            && self.lyrics.panel.local_job.is_none()
            && !(self.lyrics.job.requested().is_some()
                && self
                    .lyrics
                    .query
                    .as_ref()
                    .zip(self.lyrics.panel.query.as_ref())
                    .is_some_and(|(manual, panel)| manual.track.id == panel.track.id))
        {
            self.lyrics.panel.due = None;
            self.request_panel_lyrics(false);
        }
        let data = self.data_directory();
        let store = self.store.try_clone();
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.lyrics.panel.job.start(|request| {
            let request = request.clone();
            background(runtime, move || run_request(&data, &mut store?, &request))
        });
    }

    fn request_panel_lyrics(&mut self, force: bool) {
        let Some(query) = self.lyrics.panel.query.clone() else {
            return;
        };
        self.lyrics.panel.cancel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (progress, receiver) = mpsc::channel();
        let mut preferences = self.settings.preferences.lyrics.clone();
        preferences.enabled &= !self.lyrics.network_paused;
        self.lyrics.panel.results.clear();
        self.lyrics.panel.selected = None;
        self.lyrics.panel.choosing = false;
        self.lyrics.panel.cursor = 0;
        self.lyrics.panel.reports.clear();
        self.lyrics.panel.error = None;
        self.lyrics.panel.offset = 0;
        self.lyrics.panel.follow = true;
        self.lyrics.panel.progress = Some(receiver);
        self.lyrics.panel.cancel = Some(cancel.clone());
        self.lyrics.panel.job.request(Request {
            query,
            force,
            cancel,
            progress,
            preferences,
            excluded: self.lyrics.panel.excluded.clone(),
        });
    }

    pub(super) fn open_panel_lyrics(&mut self) -> Result<()> {
        let query = self
            .lyrics
            .panel
            .query
            .clone()
            .context("Начните воспроизведение композиции.")?;
        if let Some(cancel) = self.lyrics.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.lyrics.job.cancel();
        self.lyrics.progress = None;
        self.lyrics.saved = self.lyrics.panel.saved;
        self.lyrics.query = Some(query);
        self.lyrics.results = self.lyrics.panel.results.clone();
        self.lyrics.reports = self.lyrics.panel.reports.clone();
        self.lyrics.excluded = self.lyrics.panel.excluded.clone();
        self.lyrics.error = self.lyrics.panel.error.clone();
        self.view.dialog = Some(Dialog::Lyrics(LyricsView {
            content: self.lyrics.panel.lyric().is_some(),
            selected: if self.lyrics.panel.choosing {
                self.lyrics.panel.cursor
            } else {
                self.lyrics.panel.selected.unwrap_or(0)
            },
            panel_result: self.lyrics.panel_loading(),
            follow: true,
            ..LyricsView::default()
        }));
        Ok(())
    }

    pub(in crate::app) fn update_panel_from_dialog(&mut self) {
        let Some(query) = &self.lyrics.query else {
            return;
        };
        if !self
            .lyrics
            .panel
            .query
            .as_ref()
            .is_some_and(|q| q.track.id == query.track.id)
        {
            return;
        }
        let index = match &self.view.dialog {
            Some(Dialog::Lyrics(view)) => view.selected,
            _ => 0,
        };
        if index < self.lyrics.results.len() {
            self.lyrics.panel.cancel();
            self.lyrics.panel.results = self.lyrics.results.clone();
            self.lyrics.panel.selected = Some(index);
            self.lyrics.panel.cursor = index;
            self.lyrics.panel.choosing = false;
            self.lyrics.panel.excluded = self.lyrics.excluded.clone();
            self.lyrics.panel.reports = self.lyrics.reports.clone();
            self.lyrics.panel.error = None;
            self.lyrics.panel.saved = self.lyrics.saved;
            self.lyrics.panel.offset = 0;
            self.lyrics.panel.follow = true;
            self.lyrics.panel.remember();
        }
    }

    pub(in crate::app) fn lyrics_panel_action(&mut self, action: PanelAction) -> Result<()> {
        if !self.lyrics_panel_action_enabled(action) { return Ok(()); }
        self.focus_panel(crate::workspace::Panel::Lyrics);
        self.lyrics.panel.button = None;
        match action {
            PanelAction::Focus => {}
            PanelAction::Choose(index) => {
                if index < self.lyrics.panel.results.len() && !self.lyrics.panel_loading() {
                    self.lyrics.panel.selected = Some(index);
                    self.lyrics.panel.cursor = index;
                    self.lyrics.panel.choosing = false;
                    self.lyrics.panel.offset = 0;
                    self.lyrics.panel.follow = true;
                    self.lyrics.panel.remember();
                }
            }
            PanelAction::Variants => {
                self.lyrics.panel.choosing = true;
                self.lyrics.panel.cursor = self.lyrics.panel.selected.unwrap_or(0);
                self.lyrics.panel.offset = 0;
            }
            PanelAction::Content => {
                self.lyrics.panel.choosing = false;
                self.lyrics.panel.offset = 0;
                self.lyrics.panel.follow = true;
            }
            PanelAction::Open => self.open_panel_lyrics()?,
            PanelAction::Settings => self.open_settings_page(crate::app::SettingsPage::Lyrics),
            PanelAction::Sources => {
                self.open_panel_lyrics()?;
                self.lyrics_action(super::LyricsAction::Sources)?;
            }
            PanelAction::Save => {
                let track = self.lyrics.panel.query.as_ref().context("Начните воспроизведение композиции.")?.track.id;
                let lyric = self.lyrics.panel.lyric().context("Выберите найденный текст.")?.clone();
                self.store_lyrics(track, Some(lyric))?;
            }
            PanelAction::Remove => {
                anyhow::ensure!(self.lyrics.panel.saved && !self.lyrics_storage_busy(), "Нет сохраненного текста для удаления.");
                self.open_panel_lyrics()?;
                if let Some(Dialog::Lyrics(view)) = &mut self.view.dialog { view.confirm_remove = true; view.panel_confirmation = true; }
            }
            PanelAction::Close => {
                self.close_panel(crate::workspace::Panel::Lyrics)?;
            }
            PanelAction::Retry | PanelAction::Other => {
                if matches!(action, PanelAction::Retry) {
                    self.lyrics.panel.excluded.clear();
                } else if let Some(lyric) = self.lyrics.panel.lyric()
                    && let Some(provider) = Provider::ALL
                        .into_iter()
                        .find(|p| p.name() == lyric.provider)
                    && !self.lyrics.panel.excluded.contains(&provider)
                {
                    self.lyrics.panel.excluded.push(provider);
                }
                self.resume_lyrics_network();
                self.request_panel_lyrics(true);
            }
        }
        Ok(())
    }

    pub(crate) fn lyrics_panel_action_enabled(&self, action: PanelAction) -> bool {
        let panel = &self.lyrics.panel;
        let loading = self.lyrics.panel_loading();
        match action {
            PanelAction::Save => panel.lyric().is_some() && !loading && !self.lyrics_storage_busy(),
            PanelAction::Remove => panel.saved && !loading && !self.lyrics_storage_busy(),
            PanelAction::Open => panel.query.is_some() && !loading,
            PanelAction::Sources => panel.query.is_some(),
            PanelAction::Choose(index) => index < panel.results.len() && !loading,
            PanelAction::Variants => panel.results.len() > 1 && !loading,
            PanelAction::Content => panel.selected.is_some() && !loading,
            PanelAction::Retry | PanelAction::Other => panel.query.is_some() && !loading
                && Provider::ALL.into_iter().any(|provider| self.settings.preferences.lyrics.allows(provider)
                    && (matches!(action, PanelAction::Retry) || !panel.excluded.contains(&provider))
                    && (!matches!(action, PanelAction::Other) || panel.lyric().is_none_or(|lyric| lyric.provider != provider.name()))),
            PanelAction::Focus | PanelAction::Settings | PanelAction::Close => true,
        }
    }

    pub(crate) fn lyrics_panel_button_focused(&self, action: PanelAction) -> bool {
        self.view.dialog.is_none() && self.view.toolbar_selected.is_none() && self.lyrics.panel.focused
            && !self.view.layout_editing && self.lyrics.panel.button == Some(action)
    }

    pub(in crate::app) fn lyrics_command(&mut self, action: Action) -> Result<()> {
        let command = match action {
            Action::LyricsSave => PanelAction::Save,
            Action::LyricsRemove => PanelAction::Remove,
            Action::LyricsRetry => PanelAction::Retry,
            Action::LyricsOther => PanelAction::Other,
            Action::LyricsVariants if self.lyrics.panel.choosing && self.lyrics.panel.selected.is_some() => PanelAction::Content,
            Action::LyricsVariants => PanelAction::Variants,
            Action::LyricsSources => PanelAction::Sources,
            Action::LyricsSettings => PanelAction::Settings,
            _ => return Ok(()),
        };
        self.lyrics_panel_action(command)
    }

    pub(in crate::app) fn lyrics_command_enabled(&self, action: Action) -> bool {
        if action == Action::LyricsSettings { return true; }
        if !self.settings.preferences.lyrics.show_panel || !self.lyrics.panel.query.as_ref().zip(self.playback.current.as_ref()).is_some_and(|(query, track)| query.track.id == track.id) { return false; }
        let command = match action {
            Action::LyricsSave => PanelAction::Save,
            Action::LyricsRemove => PanelAction::Remove,
            Action::LyricsRetry => PanelAction::Retry,
            Action::LyricsOther => PanelAction::Other,
            Action::LyricsVariants if self.lyrics.panel.choosing && self.lyrics.panel.selected.is_some() => PanelAction::Content,
            Action::LyricsVariants => PanelAction::Variants,
            Action::LyricsSources => PanelAction::Sources,
            _ => return false,
        };
        self.lyrics_panel_action_enabled(command)
    }
    pub(in crate::app) fn scroll_lyrics_panel(&mut self, delta: i64) {
        let panel = &mut self.lyrics.panel;
        if panel.choosing && !panel.results.is_empty() {
            panel.cursor = super::super::bounded(panel.cursor, delta, panel.results.len());
            return;
        }
        panel.follow = false;
        panel.offset = super::super::bounded(panel.offset, delta, panel.scroll_max + 1);
    }

    pub(in crate::app) fn lyrics_panel_key(&mut self, key: KeyPress) -> Result<bool> {
        if self.view.dialog.is_some()
            || self.view.toolbar_selected.is_some()
            || self.view.layout_editing
            || self.view.workspace.gesture.is_some()
            || !self.lyrics.panel.focused
            || self.lyrics.panel.area.is_empty()
        {
            return Ok(false);
        }
        if key.ctrl || key.alt { return Ok(false); }
        let key = key.normalized();
        let command = match key.key {
            Key::Char('s') => Some(PanelAction::Save),
            Key::Delete => Some(PanelAction::Remove),
            Key::Char('r') => Some(PanelAction::Retry),
            Key::Char('n') => Some(PanelAction::Other),
            Key::Char('i') => Some(PanelAction::Sources),
            Key::Char('k') => Some(PanelAction::Settings),
            Key::Char('v') if self.lyrics.panel.choosing && self.lyrics.panel.selected.is_some() => Some(PanelAction::Content),
            Key::Char('v') => Some(PanelAction::Variants),
            _ => None,
        };
        if let Some(command) = command { self.lyrics_panel_action(command)?; return Ok(true); }
        if matches!(key.key, Key::Left | Key::Right) {
            let buttons = &self.lyrics.panel.buttons;
            if !buttons.is_empty() {
                let current = self.lyrics.panel.button.and_then(|action| buttons.iter().position(|value| *value == action));
                let index = match (current, key.key) {
                    (Some(index), Key::Left) => (index + buttons.len() - 1) % buttons.len(),
                    (Some(index), _) => (index + 1) % buttons.len(),
                    (None, Key::Left) => buttons.len() - 1,
                    (None, _) => 0,
                };
                self.lyrics.panel.button = Some(buttons[index]);
                self.view.pointer = ratatui::layout::Position::new(u16::MAX, u16::MAX);
            }
            return Ok(true);
        }
        if let Some(button) = self.lyrics.panel.button {
            match key.key {
                Key::Enter | Key::Char(' ') => { self.lyrics_panel_action(button)?; return Ok(true); }
                Key::Escape | Key::Up => { self.lyrics.panel.button = None; return Ok(true); }
                _ => {}
            }
        }
        let delta = match key.key {
            Key::Up => -1,
            Key::Down => 1,
            Key::PageUp => -(self.lyrics.panel.page_size.max(1) as i64),
            Key::PageDown => self.lyrics.panel.page_size.max(1) as i64,
            Key::Home => i64::MIN,
            Key::End => i64::MAX,
            Key::Enter => {
                if self.lyrics.panel.choosing {
                    self.lyrics_panel_action(PanelAction::Choose(self.lyrics.panel.cursor))?;
                } else if self.lyrics.panel.results.len() > 1 {
                    self.lyrics_panel_action(PanelAction::Variants)?;
                }
                return Ok(true);
            }
            Key::Escape => {
                if self.lyrics.panel.choosing && self.lyrics.panel.selected.is_some() {
                    self.lyrics_panel_action(PanelAction::Content)?;
                } else {
                    self.focus_panel(crate::workspace::Panel::Tracks);
                }
                return Ok(true);
            }
            Key::Tab => {
                self.lyrics.panel.focused = false;
                self.lyrics.panel.button = None;
                return Ok(false);
            }
            _ => return Ok(false),
        };
        if key.ctrl || key.alt {
            return Ok(false);
        }
        self.scroll_lyrics_panel(delta);
        Ok(true)
    }
}
