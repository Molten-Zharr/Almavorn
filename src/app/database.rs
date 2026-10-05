use super::{App, Dialog, Focus};
use crate::{
    model::{Mode, Playlist},
    store::{Change, Store},
};
use anyhow::{Context, Result, ensure};
use tokio::{runtime::Runtime, sync::oneshot};

pub(super) type Background<T> = oneshot::Receiver<Result<T>>;

pub(super) fn background<T: Send + 'static>(
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
        self.database.is_some() || self.library_revision != self.store.revision()
    }

    pub(super) fn save_settings(&mut self) -> Result<()> {
        self.settings.sync_active_profile();
        self.pending_settings = Some(self.settings.clone());
        self.settings_failed = false;
        self.start_settings()
    }

    pub(super) fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("runtime is alive while the interface runs")
    }

    pub(super) fn start_settings(&mut self) -> Result<()> {
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

    pub(super) fn start_database(
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

    pub(super) fn tick_database(&mut self) -> Result<()> {
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

    pub(super) fn tick_library(&mut self) -> Result<()> {
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

    pub(super) fn tick_settings(&mut self) -> Result<()> {
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
}
