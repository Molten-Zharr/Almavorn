use super::{
    App,
    background::{Background, background, poll},
    bounded,
};
use crate::errors::AppError;
use crate::{audio::Audio, model::Entry};
use anyhow::{Context, Result, ensure};
use rodio::Source;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct PlaybackRequest {
    queue: Arc<[Entry]>,
    playlist_id: i64,
    index: usize,
    paused: bool,
}
pub(super) struct WaveformJob {
    receiver: Background<Vec<f32>>,
    generation: u64,
    pub(super) cancel: Arc<AtomicBool>,
}

impl App {
    pub(super) fn adjust_volume(&mut self, step: i16) -> Result<()> {
        let percent = (self.settings.volume * 100.0).round() as i16;
        let volume = (percent + step).clamp(0, 100) as f32 / 100.0;
        if self.settings.volume == volume {
            return Ok(());
        }
        self.settings.volume = volume;
        if let Some(audio) = &self.playback.audio {
            audio.volume(volume);
        }
        self.save_settings()
    }

    pub fn position_ms(&self) -> u64 {
        if self.playback.current.is_none() {
            return 0;
        }
        self.playback.audio.as_ref().map_or(0, |audio| {
            audio.position().as_millis().min(u64::MAX as u128) as u64
        })
    }

    pub fn paused(&self) -> bool {
        self.requested_playback().map_or_else(
            || {
                self.playback.audio.as_ref().is_none_or(Audio::paused)
                    || self.playback.current.is_none()
            },
            |request| request.paused,
        )
    }
    pub fn playback_active(&self) -> bool {
        self.playback.current.is_some()
            && self.playback.audio.as_ref().is_some_and(Audio::has_track)
    }
    pub fn current_paused(&self) -> bool {
        self.playback.audio.as_ref().is_none_or(Audio::paused)
    }
    fn requested_playback(&self) -> Option<&PlaybackRequest> {
        self.playback.preparation.requested()
    }
    pub fn preparing_playback(&self) -> bool {
        self.requested_playback().is_some()
    }
    pub(super) fn can_step_playback(&self, direction: i64) -> bool {
        let (length, index) = self
            .requested_playback()
            .map(|request| (request.queue.len(), request.index))
            .unwrap_or((self.playback.queue.len(), self.playback.queue_index));
        if direction < 0 {
            index > 0
        } else {
            index + 1 < length
        }
    }

    pub(super) fn play_selected(&mut self) -> Result<()> {
        let entry = self.entry().context(AppError::TrackSelectionRequired)?;
        let playlist = self
            .playlist()
            .context(AppError::PlaylistSelectionRequired)?;
        let playlist_id = playlist.id;
        let queue: Arc<[Entry]> = playlist.entries.clone().into();
        let index = queue
            .iter()
            .position(|queued| queued.id == entry.id)
            .context(AppError::TrackMissing)?;
        self.prepare_playback(queue, playlist_id, index)
    }
    fn prepare_playback(
        &mut self,
        queue: Arc<[Entry]>,
        playlist_id: i64,
        index: usize,
    ) -> Result<()> {
        ensure!(index < queue.len(), AppError::TrackSelectionRequired);
        let title = queue[index].track.title.clone();
        self.cancel_waveform();
        self.playback.preparation.request(PlaybackRequest {
            queue,
            playlist_id,
            index,
            paused: false,
        });
        self.start_playback_job();
        self.message(format!(
            "{}: {}",
            self.text("Preparing audio", "Подготовка аудио"),
            title
        ));
        Ok(())
    }
    fn start_playback_job(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.playback.preparation.start(|request| {
            let path = request.queue[request.index].track.path.clone();
            background(runtime, move || Audio::prepare(&path))
        });
    }
    pub(super) fn tick_playback(&mut self) -> Result<()> {
        let Some(completed) = self.playback.preparation.poll() else {
            return Ok(());
        };
        self.start_playback_job();
        if !completed.current {
            return Ok(());
        }
        let request = completed.request;
        let source = match completed.result {
            Ok(source) => source,
            Err(error) => {
                if !self.playback.audio.as_ref().is_some_and(Audio::has_track) {
                    self.stop_playback();
                }
                return Err(error);
            }
        };
        let duration = source.total_duration();
        // Keep the platform audio device on its owning thread; only file access
        // and decoder preparation run in a blocking worker.
        if self.playback.audio.is_none() {
            self.playback.audio = Some(Audio::open(self.settings.volume)?);
        }
        let audio = self
            .playback
            .audio
            .as_mut()
            .context(AppError::AudioUnavailable)?;
        audio.play_prepared(source, request.paused);
        let entry = &request.queue[request.index];
        let mut track = entry.track.clone();
        if let Some(duration) = duration {
            track.duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        }
        self.playback.playing_entry = Some(entry.id);
        self.playback.playing_playlist = Some(request.playlist_id);
        self.playback.queue = request.queue;
        self.playback.queue_index = request.index;
        self.playback.current = Some(track.clone());
        self.playback.waveform = None;
        self.playback.pending_waveform =
            Some((track.path.clone(), self.playback.preparation.generation()));
        self.start_waveform_job();
        self.message(format!(
            "{}: {}",
            self.text("Playing", "Воспроизведение"),
            track.title
        ));
        Ok(())
    }
    pub(super) fn toggle_playback(&mut self) -> Result<()> {
        let request = self.playback.preparation.requested_mut();
        if let Some(request) = request {
            request.paused = !request.paused;
            if let Some(audio) = &self.playback.audio {
                audio.set_paused(request.paused);
            }
        } else if self.playback.current.is_some() {
            if let Some(audio) = &self.playback.audio {
                audio.toggle();
            }
        } else {
            self.play_selected()?;
        }
        Ok(())
    }
    pub(super) fn stop_playback(&mut self) {
        self.playback.preparation.cancel();
        if let Some(audio) = &mut self.playback.audio {
            audio.stop();
        }
        self.playback.current = None;
        self.playback.playing_entry = None;
        self.playback.playing_playlist = None;
        self.playback.waveform = None;
        self.cancel_waveform();
        self.playback.queue = Arc::from([]);
        self.playback.queue_index = 0;
        self.message(
            self.text("Playback stopped", "Воспроизведение остановлено")
                .into(),
        );
    }

    pub(super) fn next(&mut self, direction: i64, explicit: bool) -> Result<()> {
        let Some((queue, playlist_id, index)) = self
            .requested_playback()
            .map(|request| (request.queue.clone(), request.playlist_id, request.index))
            .or_else(|| {
                self.playback
                    .playing_playlist
                    .map(|id| (self.playback.queue.clone(), id, self.playback.queue_index))
            })
        else {
            return Ok(());
        };
        if direction > 0 && index + 1 >= queue.len() {
            if !explicit {
                self.stop_playback();
            }
            return Ok(());
        }
        if queue.is_empty() {
            return Ok(());
        }
        let index = bounded(index, direction, queue.len());
        self.prepare_playback(queue, playlist_id, index)
    }

    pub(super) fn cancel_waveform(&mut self) {
        self.playback.pending_waveform = None;
        if let Some(job) = &self.playback.waveform_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn start_waveform_job(&mut self) {
        if self.playback.waveform_job.is_none()
            && let Some((path, generation)) = self.playback.pending_waveform.take()
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            self.playback.waveform_job = Some(WaveformJob {
                receiver: background(self.runtime(), move || {
                    Audio::waveform(&path, &worker_cancel)
                }),
                generation,
                cancel,
            });
        }
    }

    pub fn preparing_waveform(&self) -> bool {
        self.playback.pending_waveform.is_some()
            || self.playback.waveform_job.as_ref().is_some_and(|job| {
                job.generation == self.playback.preparation.generation()
                    && !job.cancel.load(Ordering::Relaxed)
            })
    }

    pub(super) fn tick_waveform(&mut self) {
        let Some(result) = self
            .playback
            .waveform_job
            .as_mut()
            .and_then(|job| poll(&mut job.receiver))
        else {
            return;
        };
        let job = self
            .playback
            .waveform_job
            .take()
            .expect("waveform job exists");
        if job.generation == self.playback.preparation.generation()
            && self.playback.current.is_some()
            && !job.cancel.load(Ordering::Relaxed)
        {
            match result {
                Ok(waveform) => self.playback.waveform = Some(waveform),
                Err(error) => {
                    self.playback.waveform = Some(Vec::new());
                    self.failure(
                        error.context(self.text("Waveform unavailable", "Аудиоволна недоступна")),
                    );
                }
            }
        }
        self.start_waveform_job();
    }
}
