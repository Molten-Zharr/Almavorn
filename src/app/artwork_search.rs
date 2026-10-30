use super::{
    App, Dialog,
    background::{LatestJob, background},
};
use crate::{
    artwork::{
        ArtworkImage,
        search::{self, Candidate, FileSnapshot, SearchProgress, SearchResult},
    },
    input::{Key, KeyPress},
    model::Track,
};
use anyhow::Result;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone)]
struct SearchRequest {
    track: Track,
    cancel: Arc<AtomicBool>,
    progress: mpsc::Sender<SearchProgress>,
}

#[derive(Clone, Copy)]
pub enum ArtworkSearchAction {
    StartAll,
    StopAll,
    RefreshTracks,
    Manual,
    Search,
    Track(usize),
    Select(usize),
    Save,
    ConfirmSave,
    CancelSave,
    Details,
    CloseDetails,
    ScrollDetails(i16),
}

pub(crate) struct Preview {
    pub index: usize,
    pub image: Arc<ArtworkImage>,
    bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
pub(crate) struct ArtworkManualState {
    pub manual_mode: bool,
    pub tracks: Vec<Track>,
    pub track_index: usize,
    pub track: Option<Track>,
    pub results: Vec<Candidate>,
    pub selected: usize,
    pub truncated: bool,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub warning: Option<String>,
    pub confirm_save: bool,
    pub confirmation_visible: bool,
    pub details: bool,
    pub details_offset: usize,
    pub progress: Option<SearchProgress>,
    snapshot: Option<FileSnapshot>,
    search: LatestJob<SearchRequest, SearchResult>,
    preview_job: LatestJob<(usize, Candidate), (Vec<u8>, ArtworkImage)>,
    save_job: LatestJob<
        (Track, Arc<Vec<u8>>, Arc<Mutex<bool>>, FileSnapshot),
        Option<std::path::PathBuf>,
    >,
    allowed: Option<Arc<Mutex<bool>>>,
    search_cancel: Option<Arc<AtomicBool>>,
    progress_receiver: Option<mpsc::Receiver<SearchProgress>>,
}

impl App {
    pub(crate) fn artwork_manual_searching(&self) -> bool {
        self.artwork_manual.search.requested().is_some()
    }
    pub(crate) fn artwork_manual_loading(&self) -> bool {
        self.artwork_manual.preview_job.requested().is_some()
    }
    pub(crate) fn artwork_manual_saving(&self) -> bool {
        self.artwork_manual.save_job.is_running()
    }

    pub(super) fn open_artwork_search(&mut self) {
        if !self.artwork_manual_saving() {
            self.cancel_artwork_manual();
            self.artwork_manual.manual_mode = false;
            self.artwork_manual.details = false;
            self.artwork_manual.confirm_save = false;
            if !self.artwork_batch.started {
                self.refresh_artwork_tracks();
            }
        }
        self.view.dialog = Some(Dialog::ArtworkSearch);
    }

    fn refresh_artwork_tracks(&mut self) {
        if self.artwork_batch.running || self.artwork_manual_saving() {
            return;
        }
        self.cancel_artwork_manual();
        let mut tracks: Vec<_> = self
            .playlist()
            .into_iter()
            .flat_map(|playlist| &playlist.entries)
            .filter(|entry| self.library.marked.contains(&entry.id))
            .map(|entry| entry.track.clone())
            .collect();
        if tracks.is_empty() {
            tracks.extend(self.entry().map(|entry| entry.track.clone()));
        }
        let mut paths = std::collections::BTreeSet::new();
        tracks.retain(|track| paths.insert(track.path.clone()));
        let track = tracks.first().cloned();
        self.artwork_manual = ArtworkManualState {
            tracks,
            track,
            ..Default::default()
        };
        self.artwork_batch = Default::default();
    }

    pub(super) fn close_artwork_search(&mut self) -> bool {
        if self.artwork_manual.details {
            self.artwork_manual.details = false;
            return true;
        }
        if self.artwork_manual.confirm_save {
            self.artwork_manual.confirm_save = false;
            self.artwork_manual.confirmation_visible = false;
            return true;
        }
        if self.artwork_manual_saving() {
            self.message(
                self.text("Wait for the cover to save", "Дождитесь сохранения обложки")
                    .into(),
            );
            return true;
        }
        self.cancel_artwork_manual();
        if self.artwork_manual.manual_mode {
            self.artwork_manual.manual_mode = false;
            return true;
        }
        false
    }

    pub(super) fn cancel_artwork_manual(&mut self) {
        if let Some(cancel) = self.artwork_manual.search_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.artwork_manual.progress_receiver = None;
        if let Some(allowed) = self.artwork_manual.allowed.take() {
            *allowed.lock().unwrap_or_else(|error| error.into_inner()) = false;
        }
        self.artwork_manual.search.cancel();
        self.artwork_manual.preview_job.cancel();
        self.artwork_manual.save_job.cancel();
    }

    pub(super) fn artwork_search_key(&mut self, key: KeyPress) -> Result<()> {
        if key.ctrl || key.alt {
            return Ok(());
        }
        if self.artwork_manual.details {
            let delta = match key.key {
                Key::Up => -1,
                Key::Down => 1,
                Key::PageUp => -8,
                Key::PageDown => 8,
                _ => 0,
            };
            if delta != 0 {
                self.artwork_search_action(ArtworkSearchAction::ScrollDetails(delta))?;
            }
            return Ok(());
        }
        if self.artwork_manual.confirm_save {
            match key.key {
                Key::Enter => self.artwork_search_action(ArtworkSearchAction::ConfirmSave)?,
                Key::Char('n' | 'N' | 'н' | 'Н') => {
                    self.artwork_search_action(ArtworkSearchAction::CancelSave)?
                }
                _ => {}
            }
            return Ok(());
        }
        if !self.artwork_manual.manual_mode {
            let action = match key.normalized().key {
                Key::Char('r') => Some(ArtworkSearchAction::StartAll),
                Key::Char('c') => Some(ArtworkSearchAction::StopAll),
                Key::Char('u') => Some(ArtworkSearchAction::RefreshTracks),
                Key::Char('m') | Key::Enter => Some(ArtworkSearchAction::Manual),
                Key::Char('i') => Some(ArtworkSearchAction::Details),
                Key::Up => {
                    self.artwork_search_move(-1)?;
                    None
                }
                Key::Down => {
                    self.artwork_search_move(1)?;
                    None
                }
                Key::PageUp => {
                    self.artwork_search_move(-8)?;
                    None
                }
                Key::PageDown => {
                    self.artwork_search_move(8)?;
                    None
                }
                _ => None,
            };
            if let Some(action) = action {
                self.artwork_search_action(action)?;
            }
            return Ok(());
        }
        match key.normalized().key {
            Key::Char('r') => self.artwork_search_action(ArtworkSearchAction::Search)?,
            Key::Char('s') => self.artwork_search_action(ArtworkSearchAction::Save)?,
            Key::Char('i') => self.artwork_search_action(ArtworkSearchAction::Details)?,
            Key::Enter => self.artwork_search_action(ArtworkSearchAction::Save)?,
            Key::Up => self.artwork_search_move(-1)?,
            Key::Down => self.artwork_search_move(1)?,
            Key::PageUp => self.artwork_search_move(-8)?,
            Key::PageDown => self.artwork_search_move(8)?,
            Key::Left => self.artwork_search_action(ArtworkSearchAction::Track(
                self.artwork_manual.track_index.saturating_sub(1),
            ))?,
            Key::Right => self.artwork_search_action(ArtworkSearchAction::Track(
                self.artwork_manual.track_index.saturating_add(1),
            ))?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn artwork_search_move(&mut self, delta: i64) -> Result<()> {
        if !self.artwork_manual.manual_mode {
            let len = self.artwork_manual.tracks.len();
            if len > 0 {
                let index = super::bounded(self.artwork_manual.track_index, delta, len);
                self.artwork_search_action(ArtworkSearchAction::Track(index))?;
            }
            return Ok(());
        }
        let len = self.artwork_manual.results.len();
        if len > 0 {
            let next = super::bounded(self.artwork_manual.selected, delta, len);
            if next != self.artwork_manual.selected {
                self.artwork_search_action(ArtworkSearchAction::Select(next))?;
            }
        }
        Ok(())
    }

    pub(super) fn artwork_search_action(&mut self, action: ArtworkSearchAction) -> Result<()> {
        if self.artwork_batch.running
            && matches!(
                action,
                ArtworkSearchAction::Search
                    | ArtworkSearchAction::Select(_)
                    | ArtworkSearchAction::Save
                    | ArtworkSearchAction::ConfirmSave
                    | ArtworkSearchAction::Manual
                    | ArtworkSearchAction::RefreshTracks
            )
        {
            return Ok(());
        }
        match action {
            ArtworkSearchAction::StartAll => self.start_artwork_batch(),
            ArtworkSearchAction::StopAll => self.cancel_artwork_batch(),
            ArtworkSearchAction::RefreshTracks => self.refresh_artwork_tracks(),
            ArtworkSearchAction::Manual => {
                if self.artwork_manual.track.is_some() && !self.artwork_manual_saving() {
                    self.artwork_manual.manual_mode = true;
                    return self.artwork_search_action(ArtworkSearchAction::Search);
                }
            }
            ArtworkSearchAction::Track(index) => {
                if self.artwork_manual_saving()
                    || index >= self.artwork_manual.tracks.len()
                    || index == self.artwork_manual.track_index
                {
                    return Ok(());
                }
                self.cancel_artwork_manual();
                self.artwork_manual.track_index = index;
                self.artwork_manual.track = self.artwork_manual.tracks.get(index).cloned();
                self.artwork_manual.results.clear();
                self.artwork_manual.preview = None;
                self.artwork_manual.selected = 0;
                self.artwork_manual.truncated = false;
                self.artwork_manual.error = None;
                self.artwork_manual.message = None;
                self.artwork_manual.warning = None;
                self.artwork_manual.progress = None;
                self.artwork_manual.snapshot = None;
            }
            ArtworkSearchAction::Details => {
                self.artwork_manual.details = true;
                self.artwork_manual.details_offset = 0;
            }
            ArtworkSearchAction::CloseDetails => self.artwork_manual.details = false,
            ArtworkSearchAction::ScrollDetails(delta) => {
                self.artwork_manual.details_offset = self
                    .artwork_manual
                    .details_offset
                    .saturating_add_signed(isize::from(delta))
                    .min(self.view.dialog_scroll_max);
            }
            ArtworkSearchAction::Search => {
                if self.artwork_manual_saving() || self.artwork_manual_searching() {
                    return Ok(());
                }
                let Some(track) = self.artwork_manual.track.clone() else {
                    self.artwork_manual.error = Some(
                        self.text(
                            "Select a track in the library first",
                            "Сначала выберите композицию в библиотеке",
                        )
                        .into(),
                    );
                    return Ok(());
                };
                let snapshot = match FileSnapshot::read(&track.path) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        self.artwork_manual.error = Some(format!("{error:#}"));
                        return Ok(());
                    }
                };
                self.artwork_manual.snapshot = Some(snapshot);
                self.artwork_manual.preview_job.cancel();
                self.artwork_manual.results.clear();
                self.artwork_manual.preview = None;
                self.artwork_manual.selected = 0;
                self.artwork_manual.truncated = false;
                self.artwork_manual.error = None;
                self.artwork_manual.message = None;
                self.artwork_manual.warning = None;
                if let Some(cancel) = self.artwork_manual.search_cancel.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                let (progress, receiver) = mpsc::channel();
                self.artwork_manual.progress = None;
                self.artwork_manual.progress_receiver = Some(receiver);
                self.artwork_manual.search_cancel = Some(cancel.clone());
                self.artwork_manual.search.request(SearchRequest {
                    track,
                    cancel,
                    progress,
                });
            }
            ArtworkSearchAction::Select(index) => {
                if self.artwork_manual_saving() {
                    return Ok(());
                }
                let Some(candidate) = self.artwork_manual.results.get(index).cloned() else {
                    return Ok(());
                };
                self.artwork_manual.selected = index;
                self.artwork_manual.preview = None;
                self.artwork_manual.error = None;
                self.artwork_manual.message = None;
                self.artwork_manual.preview_job.request((index, candidate));
            }
            ArtworkSearchAction::Save => {
                if !self.artwork_manual_saving()
                    && self
                        .artwork_manual
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.index == self.artwork_manual.selected)
                {
                    self.artwork_manual.confirmation_visible = false;
                    self.artwork_manual.confirm_save = true;
                }
            }
            ArtworkSearchAction::CancelSave => {
                self.artwork_manual.confirm_save = false;
                self.artwork_manual.confirmation_visible = false;
            }
            ArtworkSearchAction::ConfirmSave => {
                if !self.artwork_manual.confirm_save
                    || !self.artwork_manual.confirmation_visible
                    || self.artwork_manual_saving()
                {
                    return Ok(());
                }
                let Some(track) = self.artwork_manual.track.clone() else {
                    return Ok(());
                };
                let Some(snapshot) = self.artwork_manual.snapshot.clone() else {
                    return Ok(());
                };
                let Some(preview) = self
                    .artwork_manual
                    .preview
                    .as_ref()
                    .filter(|preview| preview.index == self.artwork_manual.selected)
                else {
                    return Ok(());
                };
                let allowed = Arc::new(Mutex::new(true));
                self.artwork_manual.allowed = Some(allowed.clone());
                self.artwork_manual.save_job.request((
                    track,
                    preview.bytes.clone(),
                    allowed,
                    snapshot,
                ));
                self.artwork_manual.confirm_save = false;
                self.artwork_manual.error = None;
                self.artwork_manual.message = None;
            }
        }
        self.start_artwork_manual_jobs();
        Ok(())
    }

    fn start_artwork_manual_jobs(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.artwork_manual.search.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                search::manual_search(&request.track, &request.cancel, &request.progress)
            })
        });
        self.artwork_manual.preview_job.start(|(_, candidate)| {
            let candidate = candidate.clone();
            background(runtime, move || search::preview(&candidate))
        });
        self.artwork_manual
            .save_job
            .start(|(track, bytes, allowed, snapshot)| {
                let track = track.clone();
                let bytes = bytes.clone();
                let allowed = allowed.clone();
                let snapshot = snapshot.clone();
                background(runtime, move || {
                    search::save_manual(&track, &bytes, &allowed, &snapshot)
                })
            });
    }

    pub(super) fn tick_artwork_manual(&mut self) {
        if let Some(receiver) = &self.artwork_manual.progress_receiver {
            for progress in receiver.try_iter() {
                self.artwork_manual.progress = Some(progress);
            }
        }
        self.start_artwork_manual_jobs();
        if let Some(done) = self.artwork_manual.search.poll()
            && done.current
        {
            self.artwork_manual.search_cancel = None;
            self.artwork_manual.progress_receiver = None;
            match done.result {
                Ok(result) => {
                    self.artwork_manual.results = result.candidates;
                    self.artwork_manual.truncated = result.truncated;
                    self.artwork_manual.warning = result.warning;
                    if self.artwork_manual.results.is_empty() {
                        self.artwork_manual.message = Some(
                            if result.truncated {
                                self.text(
                                    "No matching releases in the checked results",
                                    "В проверенных результатах подходящих изданий нет",
                                )
                            } else {
                                self.text(
                                    "No matching releases found",
                                    "Подходящие издания не найдены",
                                )
                            }
                            .into(),
                        );
                    } else {
                        let _ = self.artwork_search_action(ArtworkSearchAction::Select(0));
                    }
                }
                Err(error) => {
                    self.artwork_manual.error = Some(format!("{error:#}"));
                }
            }
        }
        if let Some(done) = self.artwork_manual.preview_job.poll()
            && done.current
        {
            match done.result {
                Ok((bytes, image)) => {
                    self.artwork_manual.preview = Some(Preview {
                        index: done.request.0,
                        image: Arc::new(image),
                        bytes: Arc::new(bytes),
                    })
                }
                Err(error) => {
                    self.artwork_manual.error = Some(format!(
                        "{}: {error:#}",
                        self.text(
                            "Could not load artwork from Cover Art Archive",
                            "Не удалось загрузить обложку из Cover Art Archive"
                        )
                    ));
                }
            }
        }
        if let Some(done) = self.artwork_manual.save_job.poll()
            && done.current
        {
            self.artwork_manual.allowed = None;
            match done.result {
                Ok(backup) => {
                    if self
                        .playback
                        .current
                        .as_ref()
                        .is_some_and(|track| track.path == done.request.0.path)
                    {
                        self.invalidate_artwork();
                    }
                    if let Some(index) = self
                        .artwork_batch
                        .tracks
                        .iter()
                        .position(|track| track.path == done.request.0.path)
                    {
                        self.artwork_batch.reports[index] =
                            Some(search::batch::Outcome::Saved(backup.clone()));
                    }
                    self.artwork_manual.message = Some(if let Some(path) = backup {
                        format!(
                            "{}: {}",
                            self.text(
                                "Artwork saved; original copy kept",
                                "Обложка сохранена, исходная копия оставлена"
                            ),
                            path.display()
                        )
                    } else {
                        self.text(
                            "Artwork saved to the track",
                            "Обложка сохранена в композицию",
                        )
                        .into()
                    });
                }
                Err(error) => {
                    self.artwork_manual.error = Some(format!(
                        "{}: {error:#}",
                        self.text("Could not save artwork", "Не удалось сохранить обложку")
                    ));
                }
            }
        }
        self.start_artwork_manual_jobs();
    }
}
