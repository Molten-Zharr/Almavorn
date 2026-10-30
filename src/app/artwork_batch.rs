use super::{
    App,
    background::{LatestJob, background},
};
use crate::{
    artwork::search::{
        SearchProgress,
        batch::{self, Outcome, Stage},
    },
    model::{Language, Track},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

#[derive(Clone)]
struct Request {
    index: usize,
    track: Track,
    cancel: Arc<AtomicBool>,
    allowed: Arc<Mutex<bool>>,
    progress: mpsc::Sender<SearchProgress>,
    stage: mpsc::Sender<Stage>,
    language: Language,
}

#[derive(Default)]
pub(crate) struct ArtworkBatchState {
    pub tracks: Vec<Track>,
    pub reports: Vec<Option<Outcome>>,
    pub started: bool,
    pub running: bool,
    pub stopping: bool,
    pub current: usize,
    pub stage: Stage,
    pub progress: Option<SearchProgress>,
    job: LatestJob<Request, Outcome>,
    cancel: Option<Arc<AtomicBool>>,
    allowed: Option<Arc<Mutex<bool>>>,
    progress_receiver: Option<mpsc::Receiver<SearchProgress>>,
    stage_receiver: Option<mpsc::Receiver<Stage>>,
}

impl ArtworkBatchState {
    pub fn summary(&self, language: Language) -> String {
        let mut saved = 0;
        let mut existing = 0;
        let mut missing = 0;
        let mut review = 0;
        let mut failed = 0;
        let mut cancelled = 0;
        for report in self.reports.iter().flatten() {
            match report {
                Outcome::Saved(_) => saved += 1,
                Outcome::HasCover => existing += 1,
                Outcome::NotFound => missing += 1,
                Outcome::Review(_) => review += 1,
                Outcome::Failed(_) => failed += 1,
                Outcome::Cancelled => cancelled += 1,
            }
        }
        format!(
            "{}: {saved} · {}: {existing} · {}: {missing} · {}: {review} · {}: {failed} · {}: {cancelled}",
            language.text("Saved", "Сохранено"),
            language.text("Already have", "Уже есть"),
            language.text("Not found", "Не найдено"),
            language.text("Review", "Вручную"),
            language.text("Errors", "Ошибок"),
            language.text("Cancelled", "Отменено"),
        )
    }
}

impl App {
    pub(super) fn start_artwork_batch(&mut self) {
        if self.artwork_batch.running
            || self.artwork_manual_saving()
            || self.artwork_manual.tracks.is_empty()
        {
            return;
        }
        self.cancel_artwork_manual();
        self.artwork_manual.manual_mode = false;
        self.artwork_batch = ArtworkBatchState {
            reports: std::iter::repeat_with(|| None)
                .take(self.artwork_manual.tracks.len())
                .collect(),
            tracks: self.artwork_manual.tracks.clone(),
            started: true,
            running: true,
            cancel: Some(Arc::new(AtomicBool::new(false))),
            allowed: Some(Arc::new(Mutex::new(true))),
            ..Default::default()
        };
        self.queue_artwork_batch_track();
    }

    pub(super) fn cancel_artwork_batch(&mut self) {
        if !self.artwork_batch.running {
            return;
        }
        self.artwork_batch.stopping = true;
        if let Some(cancel) = &self.artwork_batch.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(allowed) = &self.artwork_batch.allowed {
            *allowed.lock().unwrap_or_else(|error| error.into_inner()) = false;
        }
    }

    fn queue_artwork_batch_track(&mut self) {
        let index = self.artwork_batch.current;
        let (progress, progress_receiver) = mpsc::channel();
        let (stage, stage_receiver) = mpsc::channel();
        self.artwork_batch.progress = None;
        self.artwork_batch.stage = Stage::Checking;
        self.artwork_batch.progress_receiver = Some(progress_receiver);
        self.artwork_batch.stage_receiver = Some(stage_receiver);
        self.artwork_batch.job.request(Request {
            index,
            track: self.artwork_batch.tracks[index].clone(),
            cancel: self
                .artwork_batch
                .cancel
                .as_ref()
                .expect("active batch has a cancellation flag")
                .clone(),
            allowed: self
                .artwork_batch
                .allowed
                .as_ref()
                .expect("active batch has a write guard")
                .clone(),
            progress,
            stage,
            language: self.settings.preferences.language,
        });
    }

    pub(super) fn tick_artwork_batch(&mut self) {
        if let Some(receiver) = &self.artwork_batch.progress_receiver {
            for progress in receiver.try_iter() {
                self.artwork_batch.progress = Some(progress);
            }
        }
        if let Some(receiver) = &self.artwork_batch.stage_receiver {
            for stage in receiver.try_iter() {
                self.artwork_batch.stage = stage;
            }
        }
        if let Some(done) = self.artwork_batch.job.poll()
            && done.current
        {
            let outcome = match done.result {
                Ok(outcome) => outcome,
                Err(_) if done.request.cancel.load(Ordering::Relaxed) => Outcome::Cancelled,
                Err(error) => Outcome::Failed(format!("{error:#}")),
            };
            if matches!(&outcome, Outcome::Saved(_))
                && self
                    .playback
                    .current
                    .as_ref()
                    .is_some_and(|track| track.path == done.request.track.path)
            {
                self.invalidate_artwork();
            }
            self.artwork_batch.reports[done.request.index] = Some(outcome);
            self.artwork_batch.progress_receiver = None;
            self.artwork_batch.stage_receiver = None;
            self.artwork_batch.current = done.request.index + 1;
            if self.artwork_batch.stopping
                || self.artwork_batch.current >= self.artwork_batch.tracks.len()
            {
                self.artwork_batch.running = false;
                for report in &mut self.artwork_batch.reports {
                    if report.is_none() {
                        *report = Some(Outcome::Cancelled);
                    }
                }
                self.artwork_batch.cancel = None;
                self.artwork_batch.allowed = None;
                self.message(format!(
                    "{}: {}",
                    self.text("Artwork search finished", "Поиск обложек завершен"),
                    self.artwork_batch
                        .summary(self.settings.preferences.language)
                ));
            } else {
                self.queue_artwork_batch_track();
            }
        }
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.artwork_batch.job.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                batch::process(
                    &request.track,
                    &request.cancel,
                    &request.allowed,
                    &request.progress,
                    &request.stage,
                    request.language,
                )
            })
        });
    }
}
