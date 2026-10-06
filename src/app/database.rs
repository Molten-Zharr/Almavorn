use super::background::{Background, background, poll};
use super::{App, Dialog, Focus};
use crate::errors::AppError;
use crate::{
    model::{Mode, Playlist},
    store::{Change, Store},
};
use anyhow::{Result, ensure};
use tokio::runtime::Runtime;

pub(super) enum DatabaseOutcome {
    Changed(Change),
    Created(Change),
    Renamed,
    Restored { change: Change, redo: bool },
}
pub(super) struct DatabaseJob {
    receiver: Background<DatabaseOutcome>,
    recovery: Option<Dialog>,
    pub(super) undo: Option<(Change, bool)>,
    selection: Option<i64>,
    mode: Mode,
}
pub(super) struct LibraryJob {
    receiver: Background<Vec<Playlist>>,
    revision: u64,
}

impl App {
    pub(super) fn database_busy(&self) -> bool {
        self.library.database.is_some() || self.library.revision != self.store.revision()
    }

    pub(super) fn save_settings(&mut self) -> Result<()> {
        self.settings.sync_active_profile();
        self.persistence.pending = Some(self.settings.clone());
        self.persistence.failed = false;
        self.start_settings()
    }

    pub(super) fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("runtime is alive while the interface runs")
    }

    pub(super) fn start_settings(&mut self) -> Result<()> {
        if !self.persistence.failed
            && self.persistence.job.is_none()
            && let Some(settings) = &self.persistence.pending
        {
            let mut store = self.store.try_clone()?;
            let settings = settings.clone();
            self.persistence.job = Some(background(self.runtime(), move || {
                store.save_settings(&settings)
            }));
            self.persistence.pending = None;
        }
        Ok(())
    }

    pub(super) fn start_database(
        &mut self,
        recovery: Option<Dialog>,
        operation: impl FnOnce(&mut Store) -> Result<DatabaseOutcome> + Send + 'static,
    ) -> Result<()> {
        ensure!(!self.database_busy(), AppError::LibraryBusy);
        let mut store = self.store.try_clone()?;
        self.library.database = Some(DatabaseJob {
            receiver: background(self.runtime(), move || operation(&mut store)),
            recovery,
            undo: None,
            selection: self.library.selected_playlist,
            mode: self.settings.mode,
        });
        Ok(())
    }

    pub(super) fn tick_database(&mut self) -> Result<()> {
        let Some(result) = self
            .library
            .database
            .as_mut()
            .and_then(|job| poll(&mut job.receiver))
        else {
            return Ok(());
        };
        let job = self
            .library
            .database
            .take()
            .expect("database operation is present");
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
                if self.library.selected_playlist == job.selection && self.settings.mode == job.mode
                {
                    self.library.pending_selection = new.map(|id| (id, job.mode));
                }
            }
            Ok(DatabaseOutcome::Renamed) => {}
            Ok(DatabaseOutcome::Restored { change, redo }) => {
                let history = self.library.histories.entry(change.scope).or_default();
                if redo {
                    history.0.push(change);
                } else {
                    history.1.push(change);
                }
            }
            Err(error) => {
                if let Some((change, redo)) = job.undo {
                    let history = self.library.histories.entry(change.scope).or_default();
                    if redo {
                        history.1.push(change);
                    } else {
                        history.0.push(change);
                    }
                }
                if let Some(dialog) = job.recovery {
                    self.view.dialog = Some(dialog);
                }
                if self.library.import.is_none() {
                    self.validate_history();
                }
                return Err(error);
            }
        }
        self.message(self.text("Library updated", "Библиотека обновлена").into());
        if self.library.revision == self.store.revision() && self.library.import.is_none() {
            self.validate_history();
        }
        Ok(())
    }

    pub(super) fn tick_library(&mut self) -> Result<()> {
        // A read can finish before the corresponding command result arrives.
        // Apply its deferred selection even when no additional reload is required.
        if self.library.revision == self.store.revision()
            && let Some((id, mode)) = self.library.pending_selection.take()
            && self.settings.mode == mode
        {
            self.library.selected_playlist = Some(id);
            self.view.focus = Focus::Tracks;
            self.refresh()?;
        }
        let result = self
            .library
            .load
            .as_mut()
            .and_then(|job| poll(&mut job.receiver));
        if let Some(result) = result {
            let job = self.library.load.take().expect("library read is present");
            let playlists = result?;
            if job.revision == self.store.revision() {
                self.library.playlists = playlists;
                self.library.revision = job.revision;
                if let Some((id, mode)) = self.library.pending_selection.take()
                    && self.settings.mode == mode
                {
                    self.library.selected_playlist = Some(id);
                    self.view.focus = Focus::Tracks;
                }
                self.refresh()?;
                if self.library.database.is_none() && self.library.import.is_none() {
                    self.validate_history();
                }
            }
        }
        if self.library.load.is_none() && self.library.revision != self.store.revision() {
            let store = self.store.try_clone()?;
            self.library.load = Some(LibraryJob {
                revision: store.revision(),
                receiver: background(self.runtime(), move || store.playlists()),
            });
        }
        Ok(())
    }

    pub(super) fn tick_settings(&mut self) -> Result<()> {
        let result = self.persistence.job.as_mut().and_then(poll);
        if let Some(result) = result {
            self.persistence.job = None;
            if let Err(error) = result {
                self.persistence
                    .pending
                    .get_or_insert_with(|| self.settings.clone());
                self.persistence.failed = true;
                return Err(error);
            }
        }
        self.start_settings()
    }
}
