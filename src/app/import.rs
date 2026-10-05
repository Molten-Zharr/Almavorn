use super::{
    App,
    database::{Background, background},
};
use crate::{
    media,
    model::{Mode, PlaylistKind},
    store::Change,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::oneshot;

struct ImportOutcome {
    change: Change,
    errors: Vec<String>,
    skipped: usize,
}
pub(super) struct ImportJob {
    receiver: Background<ImportOutcome>,
    target: i64,
    pub(super) cancel: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
}

impl App {
    pub fn busy(&self) -> bool {
        self.import.is_some() || self.database_busy()
    }

    pub fn importing(&self) -> bool {
        self.import.is_some()
    }

    pub fn progress(&self) -> usize {
        self.import
            .as_ref()
            .map_or(0, |job| job.progress.load(Ordering::Relaxed))
    }

    pub(super) fn import_target(&self) -> Option<i64> {
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
}

impl App {
    pub(super) fn poll_import(&mut self) -> Result<()> {
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
        Ok(())
    }
}
