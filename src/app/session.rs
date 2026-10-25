use super::{
    App, Focus, Sort,
    background::{Background, background, poll},
    services::ServiceSession,
};
use crate::model::Mode;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
const RETRY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub(super) struct SessionPersistence {
    job: Option<Background<()>>,
    writing: Option<String>,
    saved: Option<String>,
    checkpoint: Option<Instant>,
    retry_at: Option<Instant>,
    error_reported: bool,
}

#[derive(Serialize, Deserialize)]
struct Session {
    version: u8,
    mode: Mode,
    playlist: Option<i64>,
    entry: Option<i64>,
    sort: Sort,
    descending: bool,
    query: String,
    focus: Focus,
    playlist_offset: usize,
    track_offset: usize,
    marks: Vec<i64>,
    playback: Option<PlaybackSession>,
}

#[derive(Serialize, Deserialize)]
enum PlaybackSession {
    Local(LocalSession),
    Service(ServiceSession),
}

#[derive(Serialize, Deserialize)]
pub(super) struct SavedEntry {
    pub entry: i64,
    pub track: i64,
}

#[derive(Serialize, Deserialize)]
pub(super) struct LocalSession {
    pub playlist: i64,
    pub current: SavedEntry,
    pub position_ms: u64,
    pub paused: bool,
    pub marked: Option<Vec<SavedEntry>>,
}

impl App {
    pub(super) fn restore_session(&mut self) -> Result<()> {
        let Some(json) = self.store.session()? else {
            return Ok(());
        };
        let session: Session = serde_json::from_str(&json).context(self.text(
            "The saved playback session is damaged",
            "Сохраненный сеанс прослушивания поврежден",
        ))?;
        ensure!(
            session.version == 1,
            "{}",
            self.text(
                "This playback session was saved by a different version of Almavorn",
                "Сеанс прослушивания сохранен другой версией Almavorn",
            )
        );
        self.session.saved = Some(json);
        self.settings.mode = session.mode;
        let playlist = self
            .visible_playlists()
            .iter()
            .find(|p| Some(p.id) == session.playlist)
            .map(|p| p.id)
            .or_else(|| self.visible_playlists().first().map(|p| p.id));
        if let Some(id) = playlist {
            self.select_playlist(id);
        }
        self.focus_panel(match session.focus {
            Focus::Playlists => crate::workspace::Panel::Playlists,
            Focus::Tracks => crate::workspace::Panel::Tracks,
        });
        if playlist == session.playlist {
            self.library.sort = session.sort;
            self.library.sort_descending = session.descending;
            self.library.query = session.query;
            self.library.selected_entry = self
                .rows()
                .iter()
                .find(|entry| Some(entry.id) == session.entry)
                .map(|entry| entry.id)
                .or_else(|| self.rows().first().map(|entry| entry.id));
            let valid_marks: std::collections::HashSet<_> = self
                .playlist()
                .into_iter()
                .flat_map(|p| p.entries.iter().map(|entry| entry.id))
                .collect();
            self.library.marked = session
                .marks
                .into_iter()
                .filter(|id| valid_marks.contains(id))
                .collect();
            self.view.playlist_offset = session
                .playlist_offset
                .min(self.visible_playlists().len().saturating_sub(1));
            self.view.track_offset = session
                .track_offset
                .min(self.rows().len().saturating_mul(2).saturating_sub(1));
            self.view.playlist_scroll_selected = self.library.selected_playlist;
            self.view.track_scroll_selected = self.library.selected_entry;
        }
        match session.playback {
            Some(PlaybackSession::Local(playback)) => self.restore_local_session(playback)?,
            Some(PlaybackSession::Service(playback)) => self.restore_service_session(playback)?,
            None => {}
        }
        Ok(())
    }

    pub(super) fn session_snapshot(&self) -> Result<String> {
        let mut marks: Vec<_> = self.library.marked.iter().copied().collect();
        marks.sort_unstable();
        let playback = self
            .local_session()
            .map(PlaybackSession::Local)
            .or_else(|| self.service_session().map(PlaybackSession::Service));
        serde_json::to_string(&Session {
            version: 1,
            mode: self.settings.mode,
            playlist: self.library.selected_playlist,
            entry: self.library.selected_entry,
            sort: self.library.sort,
            descending: self.library.sort_descending,
            query: self.library.query.clone(),
            focus: self.view.focus,
            playlist_offset: self.view.playlist_offset,
            track_offset: self.view.track_offset,
            marks,
            playback,
        })
        .context("Cannot encode playback session")
    }

    pub(super) fn tick_session(&mut self) -> Result<()> {
        if let Some(result) = self.session.job.as_mut().and_then(poll) {
            self.session.job = None;
            let writing = self.session.writing.take();
            if let Err(error) = result {
                return self.session_save_failed(error);
            }
            self.session.saved = writing;
            self.session.error_reported = false;
            self.session.retry_at = None;
        }
        if self.session.job.is_some()
            || self
                .session
                .retry_at
                .is_some_and(|until| Instant::now() < until)
            || self
                .session
                .checkpoint
                .is_some_and(|last| last.elapsed() < CHECKPOINT_INTERVAL)
        {
            return Ok(());
        }
        self.session.checkpoint = Some(Instant::now());
        let result = (|| {
            let json = self.session_snapshot()?;
            if self.session.saved.as_ref() == Some(&json) {
                return Ok(());
            }
            let mut store = self.store.try_clone()?;
            self.session.writing = Some(json.clone());
            self.session.job = Some(background(self.runtime(), move || {
                store.save_session(&json)
            }));
            Ok(())
        })();
        result.or_else(|error| self.session_save_failed(error))
    }

    fn session_save_failed(&mut self, error: anyhow::Error) -> Result<()> {
        self.session.retry_at = Some(Instant::now() + RETRY_INTERVAL);
        if std::mem::replace(&mut self.session.error_reported, true) {
            return Ok(());
        }
        Err(error.context(self.text(
            "Could not save the playback session",
            "Не удалось сохранить сеанс прослушивания",
        )))
    }

    pub(super) fn shutdown_session(
        &mut self,
        runtime: &Runtime,
        deadline: Instant,
        snapshot: String,
    ) -> Result<()> {
        let wait = |job: Background<()>| {
            runtime
                .block_on(async {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), job).await
                })
                .context("Playback session save did not finish before shutdown")
                .map(|result| {
                    result
                        .context("Playback session save was interrupted")
                        .and_then(|r| r)
                })
        };
        if let Some(job) = self.session.job.take() {
            // On timeout the writer may still run. Never start a second writer
            // that could finish first and then be overwritten by the older one.
            if wait(job)?.is_ok() {
                self.session.saved = self.session.writing.take();
            }
        }
        if self.session.saved.as_ref() != Some(&snapshot) {
            let mut store = self.store.try_clone()?;
            wait(background(runtime, move || store.save_session(&snapshot)))??;
        }
        Ok(())
    }
}
