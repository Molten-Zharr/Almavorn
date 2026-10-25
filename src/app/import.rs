use super::{
    App, Dialog,
    background::{Background, background, poll},
};
use crate::errors::AppError;
use crate::{
    media,
    model::{ImportedTrack, Mode, PlaylistKind},
    store::{Change, FolderPlaylistImport},
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
    changes: Vec<Change>,
    errors: Vec<String>,
    skipped: usize,
}
enum ImportDestination {
    Existing(i64),
    Scan(Vec<PlaylistScan>),
    Create {
        name: String,
        mode: Mode,
        group_folders: bool,
    },
    CreateFolders {
        mode: Mode,
        group_folders: bool,
    },
}

struct PlaylistScan {
    id: i64,
    name: String,
    folders: Vec<PathBuf>,
    known: HashSet<PathBuf>,
    recursive: bool,
}

struct ImportedMusic {
    tracks: Vec<ImportedTrack>,
    folders: Vec<PathBuf>,
    errors: Vec<String>,
    skipped: usize,
}
pub(super) struct ImportJob {
    receiver: Background<ImportOutcome>,
    target: Option<i64>,
    recovery: Option<Dialog>,
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

    pub(crate) fn import_target(&self) -> Option<i64> {
        if self.settings.mode == Mode::Chaos
            && !self.settings.preferences.sorting_desk
            && let Some(playlist) = self
                .playlist()
                .filter(|playlist| self.playlist_editable(playlist))
        {
            return Some(playlist.id);
        }
        self.library
            .playlists
            .iter()
            .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
            .map(|playlist| playlist.id)
    }

    pub fn import_caption(&self) -> String {
        match self
            .import_target()
            .and_then(|id| self.library.playlists.iter().find(|p| p.id == id))
        {
            Some(playlist) if playlist.kind == PlaylistKind::SortingDesk => {
                self.text("Add to desk", "Добавить на стол").into()
            }
            Some(playlist) => format!("{} «{}»", self.text("Add to", "Добавить в"), playlist.name),
            None => self.text("Add music", "Добавить музыку").into(),
        }
    }

    pub fn start_import(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        self.start_import_skipping(paths, HashSet::new())
    }

    pub(super) fn start_desk_folder(&mut self, paths: Vec<PathBuf>) -> Result<()> {
        let desk = self.library.playlists.iter()
            .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
            .context(AppError::PlaylistMissing)?.id;
        self.start_import_in_playlist(paths, HashSet::new(), desk)
    }

    pub fn dropped_import_target(&self) -> Option<i64> {
        // Dropping files follows the mode, independently of the add-to-desk option.
        match self.settings.mode {
            Mode::Order => self
                .library
                .playlists
                .iter()
                .find(|playlist| playlist.kind == PlaylistKind::SortingDesk)
                .map(|playlist| playlist.id),
            Mode::Chaos => self
                .playlist()
                .filter(|playlist| {
                    playlist.mode == Mode::Chaos || playlist.kind == PlaylistKind::SortingDesk
                })
                .map(|playlist| playlist.id),
        }
    }

    pub(super) fn start_dropped_import(
        &mut self,
        paths: Vec<PathBuf>,
        playlist: Option<i64>,
    ) -> Result<()> {
        let playlist = playlist.context(AppError::PlaylistSelectionRequired)?;
        self.start_import_in_playlist(paths, HashSet::new(), playlist)
    }

    pub(super) fn start_folder_playlist(
        &mut self,
        name: String,
        folder: PathBuf,
        group_subfolders: bool,
        recovery: Option<Dialog>,
    ) -> Result<()> {
        self.start_import_into(
            vec![folder],
            HashSet::new(),
            ImportDestination::Create {
                name,
                mode: self.settings.mode,
                group_folders: group_subfolders,
            },
            group_subfolders,
            recovery,
        )
    }

    pub(super) fn start_folder_playlists(
        &mut self,
        folders: Vec<PathBuf>,
        group_subfolders: bool,
        recovery: Option<Dialog>,
    ) -> Result<()> {
        ensure!(!folders.is_empty(), AppError::FolderSelectionRequired);
        self.start_import_into(
            folders,
            HashSet::new(),
            ImportDestination::CreateFolders {
                mode: self.settings.mode,
                group_folders: group_subfolders,
            },
            group_subfolders,
            recovery,
        )
    }

    pub(super) fn scan_playlist_folders(&mut self, id: i64) -> Result<()> {
        let playlist = self
            .library
            .playlists
            .iter()
            .find(|playlist| playlist.id == id)
            .context(AppError::PlaylistMissing)?;
        self.start_playlist_scan(vec![(id, playlist.scan_folders())], None)
    }

    pub(super) fn start_playlist_scan(
        &mut self,
        choices: Vec<(i64, Vec<PathBuf>)>,
        recovery: Option<Dialog>,
    ) -> Result<()> {
        ensure!(!choices.is_empty(), AppError::FolderSelectionRequired);
        let mut scans = Vec::new();
        for (id, mut folders) in choices {
            let playlist = self.library.playlists.iter().find(|p| p.id == id)
                .context(AppError::PlaylistMissing)?;
            let available = playlist.scan_folders();
            ensure!(!folders.is_empty() && folders.iter().all(|p| available.contains(p)),
                "Выберите папки плейлиста или существующих групп сортировочного стола.");
            folders.sort();
            folders.dedup();
            scans.push(PlaylistScan {
                id,
                name: playlist.display_name(self.settings.preferences.language).to_owned(),
                folders,
                known: playlist.entries.iter().map(|entry| entry.track.path.clone()).collect(),
                recursive: playlist.kind == PlaylistKind::Normal && self.settings.preferences.scan_subfolders,
            });
        }
        self.start_import_into(Vec::new(), HashSet::new(), ImportDestination::Scan(scans), false, recovery)
    }

    fn start_import_skipping(
        &mut self,
        paths: Vec<PathBuf>,
        known: HashSet<PathBuf>,
    ) -> Result<()> {
        ensure!(self.library.import.is_none(), AppError::ImportBusy);
        let target = self
            .import_target()
            .context("Select an editable playlist or enable the sorting desk")?;
        self.start_import_in_playlist(paths, known, target)
    }

    fn start_import_in_playlist(
        &mut self,
        paths: Vec<PathBuf>,
        mut known: HashSet<PathBuf>,
        target: i64,
    ) -> Result<()> {
        known.extend(
            self.library
                .playlists
                .iter()
                .find(|playlist| playlist.id == target)
                .into_iter()
                .flat_map(|playlist| &playlist.entries)
                .map(|entry| entry.track.path.clone()),
        );
        self.start_import_into(
            paths,
            known,
            ImportDestination::Existing(target),
            self.settings.preferences.scan_subfolders,
            None,
        )
    }

    fn start_import_into(
        &mut self,
        paths: Vec<PathBuf>,
        known: HashSet<PathBuf>,
        destination: ImportDestination,
        recursive: bool,
        recovery: Option<Dialog>,
    ) -> Result<()> {
        ensure!(!self.busy(), AppError::LibraryBusy);
        let target = match &destination {
            ImportDestination::Existing(id) => Some(*id),
            ImportDestination::Scan(scans) => scans.first().map(|scan| scan.id),
            ImportDestination::Create { .. } | ImportDestination::CreateFolders { .. } => None,
        };
        let creating = matches!(destination, ImportDestination::Create { .. } | ImportDestination::CreateFolders { .. });
        let grouping = target.is_some_and(|id| self.library.playlists.iter()
            .find(|playlist| playlist.id == id).is_some_and(|playlist| playlist.groups_by_folder())) || matches!(
            &destination,
            ImportDestination::Create {
                group_folders: true,
                ..
            } | ImportDestination::CreateFolders {
                group_folders: true,
                ..
            }
        );
        let mut worker_store = self.store.try_clone()?;
        let unlocked = target.is_some_and(|id| self.playlist_unlocked(id));
        let language = self.settings.preferences.language;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let progress = Arc::new(AtomicUsize::new(0));
        let worker_progress = progress.clone();
        let receiver = background(self.runtime(), move || {
            if let ImportDestination::Scan(scans) = &destination {
                let mut changes = Vec::new();
                let mut errors = Vec::new();
                let mut skipped = 0;
                for scan in scans {
                    if worker_cancel.load(Ordering::Relaxed) { break; }
                    let result = read_music(scan.folders.clone(), scan.known.clone(), true,
                        scan.recursive, scan.recursive, &worker_cancel, &worker_progress);
                    let result = result.and_then(|music| {
                        skipped += music.skipped;
                        errors.extend(music.errors.into_iter().take(5usize.saturating_sub(errors.len())));
                        // An explicit refresh only appends to its own playlist. It never uses the desk switch.
                        worker_store.add_tracks_from_folders(scan.id, &music.tracks, &[], true)
                    });
                    match result {
                        Ok(change) => changes.push(change),
                        Err(error) => errors.push(format!("{}: {}", scan.name, crate::errors::message(&error, language))),
                    }
                }
                ensure!(!changes.is_empty(), "{}", errors.first().map(String::as_str)
                    .unwrap_or(language.text("Scan cancelled", "Поиск отменен")));
                return Ok(ImportOutcome { changes, errors, skipped });
            }
            if let ImportDestination::CreateFolders {
                mode,
                group_folders,
            } = &destination
            {
                let mut playlists = Vec::new();
                let mut errors = Vec::new();
                let mut skipped = 0;
                for folder in paths {
                    let name = folder
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context(AppError::InvalidPathEncoding)?
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .take(145)
                        .collect::<String>();
                    let music = read_music(
                        vec![folder.clone()],
                        HashSet::new(),
                        true,
                        *group_folders,
                        recursive,
                        &worker_cancel,
                        &worker_progress,
                    )
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "{} {}: {}",
                            language.text("Cannot import folder", "Не удалось добавить папку"),
                            folder.display(),
                            crate::errors::message(&error, language)
                        )
                    })?;
                    skipped += music.skipped;
                    let available = 5usize.saturating_sub(errors.len());
                    errors.extend(music.errors.into_iter().take(available));
                    playlists.push(FolderPlaylistImport {
                        name,
                        tracks: music.tracks,
                        folders: music.folders,
                    });
                }
                let change = worker_store.create_folder_playlists(
                    &playlists,
                    *mode,
                    *group_folders,
                    &worker_cancel,
                )?;
                return Ok(ImportOutcome {
                    changes: vec![change],
                    errors,
                    skipped,
                });
            }
            let ImportedMusic {
                tracks,
                folders,
                errors,
                skipped,
            } = read_music(
                paths,
                known,
                creating,
                grouping,
                recursive,
                &worker_cancel,
                &worker_progress,
            )?;
            let change = match destination {
                ImportDestination::Existing(id) => {
                    worker_store.add_tracks_from_folders(id, &tracks, &folders, unlocked)?
                }
                ImportDestination::Create {
                    name,
                    mode,
                    group_folders,
                } => worker_store.create_playlist_with_tracks(
                    &name,
                    mode,
                    &tracks,
                    &folders,
                    group_folders,
                )?,
                ImportDestination::CreateFolders { .. } | ImportDestination::Scan(_) => {
                    unreachable!("folder playlists are imported above")
                }
            };
            Ok(ImportOutcome {
                changes: vec![change],
                errors,
                skipped,
            })
        });
        self.library.import = Some(ImportJob {
            receiver,
            target,
            recovery,
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

fn read_music(
    paths: Vec<PathBuf>,
    known: HashSet<PathBuf>,
    creating: bool,
    grouping: bool,
    recursive: bool,
    cancel: &AtomicBool,
    progress: &AtomicUsize,
) -> Result<ImportedMusic> {
    let mut tracks = Vec::new();
    let mut errors = Vec::new();
    let mut skipped = 0;
    let mut candidates = Vec::new();
    let mut folders = Vec::new();
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!(AppError::ImportInterrupted);
        }
        if creating {
            ensure!(path.is_dir(), AppError::FolderNotReady);
            std::fs::read_dir(&path).context(AppError::FolderNotReady)?;
        }
        if path.is_dir() {
            match media::scan_audio_files(&path, cancel, recursive) {
                Ok(scan) => {
                    folders.push(path.canonicalize()?);
                    candidates.extend(scan.files);
                    skipped += scan.skipped;
                    let available = 5usize.saturating_sub(errors.len());
                    errors.extend(scan.errors.into_iter().take(available));
                }
                Err(error) => {
                    if creating {
                        return Err(error);
                    }
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
    if grouping {
        candidates.sort_by(|left, right| {
            left.parent()
                .cmp(&right.parent())
                .then_with(|| left.cmp(right))
        });
    }
    let mut seen = known;
    for path in candidates {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!(AppError::ImportInterrupted);
        }
        progress.fetch_add(1, Ordering::Relaxed);
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
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!(AppError::ImportInterrupted);
    }
    folders.sort();
    folders.dedup();
    Ok(ImportedMusic {
        tracks,
        folders,
        errors,
        skipped,
    })
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
            let outcome = match outcome {
                Ok(outcome) => outcome,
                Err(error) => {
                    if let Some(dialog) = job.recovery {
                        self.restore_import_dialog(dialog);
                    }
                    return Err(error);
                }
            };
            let changes = outcome.changes;
            let target = job
                .target
                .or_else(|| {
                    changes.iter().find_map(|change| change.after.playlists.iter()
                        .find(|playlist| {
                            !change
                                .before
                                .playlists
                                .iter()
                                .any(|previous| previous.id == playlist.id)
                        })
                        .map(|playlist| playlist.id))
                })
                .context(AppError::PlaylistMissing)?;
            let mode = changes.iter().flat_map(|change| &change.after.playlists)
                .find(|playlist| playlist.id == target)
                .map(|playlist| {
                    if playlist.kind == "desk" {
                        self.settings.mode
                    } else if playlist.mode == "chaos" {
                        Mode::Chaos
                    } else {
                        Mode::Order
                    }
                });
            let added: usize = changes.iter().map(|change| change
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
                )).sum();
            let created: usize = changes.iter().map(|change| change
                .after
                .playlists
                .len()
                .saturating_sub(change.before.playlists.len())).sum();
            for change in changes { self.remember(change)?; }
            if self.library.pending_selection.is_none()
                && let Some(mode) = mode
            {
                self.settings.mode = mode;
                self.library.pending_selection = Some((target, mode));
            }
            self.save_settings()?;
            let mut message = format!(
                "{}: {added}. {}: {}",
                self.text("Added", "Добавлено"),
                self.text("Skipped", "Пропущено"),
                outcome.skipped
            );
            if created > 0 {
                message = format!(
                    "{}: {created}. {message}",
                    self.text("Playlists created", "Создано плейлистов")
                );
            }
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
