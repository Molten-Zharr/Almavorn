use super::{
    App, Dialog, Target,
    background::{LatestJob, background},
};
use crate::{
    artwork::{
        ArtworkImage,
        search::{self, Candidate, SearchProgress},
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
    Search,
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
    pub track: Option<Track>,
    pub results: Vec<Candidate>,
    pub selected: usize,
    pub truncated: bool,
    pub preview: Option<Preview>,
    pub error: Option<String>,
    pub message: Option<String>,
    pub confirm_save: bool,
    pub confirmation_visible: bool,
    pub details: bool,
    pub details_offset: usize,
    pub progress: Option<SearchProgress>,
    search: LatestJob<SearchRequest, (Vec<Candidate>, bool)>,
    preview_job: LatestJob<(usize, Candidate), (Vec<u8>, ArtworkImage)>,
    save_job: LatestJob<(Track, Arc<Vec<u8>>, Arc<Mutex<bool>>), Option<std::path::PathBuf>>,
    allowed: Option<Arc<Mutex<bool>>>,
    search_cancel: Option<Arc<AtomicBool>>,
    progress_receiver: Option<mpsc::Receiver<SearchProgress>>,
}

impl App {
    pub(crate) fn artwork_manual_searching(&self) -> bool {
        self.artwork_manual.search.is_running()
    }
    pub(crate) fn artwork_manual_loading(&self) -> bool {
        self.artwork_manual.preview_job.requested().is_some()
    }
    pub(crate) fn artwork_manual_saving(&self) -> bool {
        self.artwork_manual.save_job.is_running()
    }

    pub(super) fn open_artwork_search(&mut self) {
        self.cancel_artwork_manual();
        let track = self.entry().map(|entry| entry.track.clone());
        self.artwork_manual = ArtworkManualState {
            track,
            ..Default::default()
        };
        self.view.dialog = Some(Dialog::ArtworkSearch);
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
        match key.normalized().key {
            Key::Char(' ') => self.target(Target::ToggleArtworkSearch, false)?,
            Key::Char('r') => self.artwork_search_action(ArtworkSearchAction::Search)?,
            Key::Char('s') => self.artwork_search_action(ArtworkSearchAction::Save)?,
            Key::Char('i') => self.artwork_search_action(ArtworkSearchAction::Details)?,
            Key::Enter => self.artwork_search_action(ArtworkSearchAction::Save)?,
            Key::Up => self.artwork_search_move(-1)?,
            Key::Down => self.artwork_search_move(1)?,
            Key::PageUp => self.artwork_search_move(-8)?,
            Key::PageDown => self.artwork_search_move(8)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn artwork_search_move(&mut self, delta: i64) -> Result<()> {
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
        match action {
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
                self.artwork_manual.preview_job.cancel();
                self.artwork_manual.results.clear();
                self.artwork_manual.preview = None;
                self.artwork_manual.selected = 0;
                self.artwork_manual.truncated = false;
                self.artwork_manual.error = None;
                self.artwork_manual.message = None;
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
                self.artwork_manual
                    .save_job
                    .request((track, preview.bytes.clone(), allowed));
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
            .start(|(track, bytes, allowed)| {
                let track = track.clone();
                let bytes = bytes.clone();
                let allowed = allowed.clone();
                background(runtime, move || {
                    search::save_manual(&track, &bytes, &allowed)
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
                Ok((results, truncated)) => {
                    self.artwork_manual.results = results;
                    self.artwork_manual.truncated = truncated;
                    if self.artwork_manual.results.is_empty() {
                        self.artwork_manual.message = Some(
                            if truncated {
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
                    self.artwork_manual.error = Some(format!(
                        "{}: {error:#}",
                        self.text(
                            "MusicBrainz search failed",
                            "Не удалось выполнить поиск в MusicBrainz"
                        )
                    ));
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
