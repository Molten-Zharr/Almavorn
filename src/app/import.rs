use super::{
    App,
    background::{Background, background, poll},
};
use crate::errors::AppError;
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
        self.library.import.is_some() || self.database_busy()
    }

    pub fn importing(&self) -> bool {
        self.library.import.is_some()
    }

    pub fn progress(&self) -> usize {
        self.library
            .import
            .as_ref()
            .map_or(0, |job| job.progress.load(Ordering::Relaxed))
    }

    pub(super) fn import_target(&self) -> Option<i64> {
        if self.settings.sorting_desk {
            self.library
                .playlists
                .iter()
                .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
                .map(|playlist| playlist.id)
        } else {
            self.playlist()
                .filter(|playlist| playlist.can_edit(self.view.editing))
                .map(|playlist| playlist.id)
        }
    }

    pub fn start_import(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        self.start_import_skipping(paths, HashSet::new())
    }

    pub(super) fn scan_playlist_folders(&mut self, id: i64) -> Result<()> {
        ensure!(!self.busy(), AppError::LibraryBusy);
        let playlist = self
            .library
            .playlists
            .iter()
            .find(|playlist| playlist.id == id)
            .context(AppError::PlaylistMissing)?;
        let mut folders = playlist.folders.clone();
        folders.sort();
        folders.dedup();
        ensure!(!folders.is_empty(), AppError::FolderNotReady);
        let known = playlist
            .entries
            .iter()
            .map(|entry| entry.track.path.clone())
            .collect();
        self.start_import_skipping(folders, known)
    }

    fn start_import_skipping(
        &mut self,
        paths: Vec<PathBuf>,
        mut known: HashSet<PathBuf>,
    ) -> Result<()> {
        ensure!(self.library.import.is_none(), AppError::ImportBusy);
        let target = self
            .import_target()
            .context("Select an editable playlist or enable the sorting desk")?;
        known.extend(
            self.library
                .playlists
                .iter()
                .find(|playlist| playlist.id == target)
                .into_iter()
                .flat_map(|playlist| &playlist.entries)
                .map(|entry| entry.track.path.clone()),
        );
        let mut worker_store = self.store.try_clone()?;
        let unlocked = self.view.editing;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_progress = progress.clone();
        let receiver = background(self.runtime(), move || {
            let mut tracks = Vec::new();
            let mut errors = Vec::new();
            let mut skipped = 0;
            let mut candidates = Vec::new();
            let mut folders = Vec::new();
            for path in paths {
                if worker_cancel.load(Ordering::Relaxed) {
                    anyhow::bail!(AppError::ImportInterrupted);
                }
                if path.is_dir() {
                    match media::scan_audio_files(&path, &worker_cancel) {
                        Ok(scan) => {
                            folders.push(path.canonicalize()?);
                            candidates.extend(scan.files);
                            skipped += scan.skipped;
                            let available = 5usize.saturating_sub(errors.len());
                            errors.extend(scan.errors.into_iter().take(available));
                        }
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
            let mut seen = known;
            for path in candidates {
                if worker_cancel.load(Ordering::Relaxed) {
                    anyhow::bail!(AppError::ImportInterrupted);
                }
                worker_progress.fetch_add(1, Ordering::Relaxed);
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
                            if let Some(parent) = track.path.parent()
                                && !folders.iter().any(|folder| parent.starts_with(folder))
                            {
                                folders.push(parent.to_owned());
                            }
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
            }
            if worker_cancel.load(Ordering::Relaxed) {
                anyhow::bail!(AppError::ImportInterrupted);
            }
            folders.sort();
            folders.dedup();
            let change =
                worker_store.add_tracks_from_folders(target, &tracks, &folders, unlocked)?;
            Ok(ImportOutcome {
                change,
                errors,
                skipped,
            })
        });
        self.library.import = Some(ImportJob {
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
        let outcome = self
            .library
            .import
            .as_mut()
            .and_then(|job| poll(&mut job.receiver));
        if let Some(outcome) = outcome {
            let job = self.library.import.take().expect("import is present");
            let outcome = outcome?;
            let change = outcome.change;
            let mode = change
                .after
                .playlists
                .iter()
                .find(|playlist| playlist.id == job.target)
                .map(|playlist| {
                    if playlist.kind == "desk" {
                        self.settings.mode
                    } else if playlist.mode == "chaos" {
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
            if self.library.pending_selection.is_none()
                && let Some(mode) = mode
            {
                self.settings.mode = mode;
                self.library.pending_selection = Some((job.target, mode));
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
            self.view.notice_error = !outcome.errors.is_empty();
            if self.library.revision == self.store.revision() && self.library.database.is_none() {
                self.validate_history();
            }
        }
        Ok(())
    }
}
