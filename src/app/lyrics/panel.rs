use super::{
    App, Dialog, Lyric, LyricsState, LyricsView, Provider, ProviderReport, Query, Request,
    SearchResult, background, run_request,
};
use crate::{app::background::LatestJob, model::Track};
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
}

#[derive(Default)]
pub struct LyricsPanel {
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
    key: Option<TrackKey>,
    due: Option<Instant>,
    cache: VecDeque<Cached>,
    job: LatestJob<Request, SearchResult>,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<mpsc::Receiver<ProviderReport>>,
}
#[derive(Clone, Copy)]
pub enum PanelAction {
    Open,
    Other,
    Retry,
    Settings,
    Close,
}

impl LyricsPanel {
    pub(super) fn cancel(&mut self) {
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
        self.cache.push_back(Cached {
            key,
            results: self.results.iter().take(3).cloned().collect(),
            reports: self.reports.clone(),
            excluded: self.excluded.clone(),
        });
        while self.cache.len() > 24 {
            self.cache.pop_front();
        }
    }
    pub fn lyric(&self) -> Option<&Lyric> {
        self.results.first().filter(|lyric| lyric.score >= 900)
    }
}
impl Drop for LyricsPanel {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl LyricsState {
    pub fn panel_loading(&self) -> bool {
        self.panel.job.requested().is_some() || self.panel.due.is_some()
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
                    if result.network_failed {
                        self.lyrics.network_paused = true;
                    }
                    self.lyrics.panel.results = result.matches;
                    self.lyrics.panel.error = if self.lyrics.panel.results.is_empty()
                        && !result.warnings.is_empty()
                    {
                        Some(self.text("No lyrics found. Source details are available in the full view.", "Текст не найден. Подробности по источникам доступны в полном окне.").into())
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
                if let Some(cached) = panel.cache.iter().find(|entry| entry.key == key) {
                    panel.results = cached.results.clone();
                    panel.reports = cached.reports.clone();
                    panel.excluded = cached.excluded.clone();
                } else {
                    panel.due = Some(Instant::now() + Duration::from_millis(450));
                }
            }
        }
        if self
            .lyrics
            .panel
            .due
            .is_some_and(|due| Instant::now() >= due)
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
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.lyrics.panel.job.start(|request| {
            let request = request.clone();
            background(runtime, move || run_request(&data, &request))
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
        self.lyrics.saved =
            crate::lyrics::cache_path(&self.data_directory(), query.track.id).exists();
        self.lyrics.query = Some(query);
        self.lyrics.results = self.lyrics.panel.results.clone();
        self.lyrics.reports = self.lyrics.panel.reports.clone();
        self.lyrics.excluded = self.lyrics.panel.excluded.clone();
        self.lyrics.error = self.lyrics.panel.error.clone();
        self.view.dialog = Some(Dialog::Lyrics(LyricsView {
            content: self.lyrics.panel.lyric().is_some(),
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
        if let Some(mut lyric) = self.lyrics.results.get(index).cloned() {
            lyric.score = 1000;
            self.lyrics.panel.cancel();
            self.lyrics.panel.results = vec![lyric];
            self.lyrics.panel.excluded = self.lyrics.excluded.clone();
            self.lyrics.panel.reports = self.lyrics.reports.clone();
            self.lyrics.panel.error = None;
            self.lyrics.panel.offset = 0;
            self.lyrics.panel.follow = true;
            self.lyrics.panel.remember();
        }
    }

    pub(in crate::app) fn lyrics_panel_action(&mut self, action: PanelAction) -> Result<()> {
        match action {
            PanelAction::Open => self.open_panel_lyrics()?,
            PanelAction::Settings => self.open_settings_page(crate::app::SettingsPage::Lyrics),
            PanelAction::Close => {
                self.settings.preferences.lyrics.show_panel = false;
                self.save_settings()?;
            }
            PanelAction::Retry | PanelAction::Other => {
                if matches!(action, PanelAction::Retry) {
                    self.lyrics.panel.excluded.clear();
                } else if let Some(lyric) = self.lyrics.panel.results.first()
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
    pub(in crate::app) fn scroll_lyrics_panel(&mut self, delta: i64) {
        let panel = &mut self.lyrics.panel;
        panel.follow = false;
        panel.offset = super::super::bounded(panel.offset, delta, panel.scroll_max + 1);
    }
}
