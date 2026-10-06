use super::{
    App, bounded,
    database::{Background, background},
};
use crate::{
    audio::{Audio, PreparedAudio},
    model::Entry,
};
use anyhow::{Context, Result, ensure};
use rodio::Source;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

pub(super) struct PlaybackRequest {
    queue: Arc<[Entry]>,
    playlist_id: i64,
    index: usize,
    generation: u64,
    paused: bool,
}
pub(super) struct PlaybackJob {
    receiver: Background<PreparedAudio>,
    request: PlaybackRequest,
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
        if let Some(audio) = &self.audio {
            audio.volume(volume);
        }
        self.save_settings()
    }

    pub fn position_ms(&self) -> u64 {
        if self.current.is_none() {
            return 0;
        }
        self.audio.as_ref().map_or(0, |audio| {
            audio.position().as_millis().min(u64::MAX as u128) as u64
        })
    }

    pub fn paused(&self) -> bool {
        self.requested_playback().map_or_else(
            || self.audio.as_ref().is_none_or(Audio::paused) || self.current.is_none(),
            |request| request.paused,
        )
    }
    pub fn playback_active(&self) -> bool {
        self.current.is_some() && self.audio.as_ref().is_some_and(Audio::has_track)
    }
    pub fn current_paused(&self) -> bool {
        self.audio.as_ref().is_none_or(Audio::paused)
    }
    fn requested_playback(&self) -> Option<&PlaybackRequest> {
        self.pending_playback.as_ref().or_else(|| {
            self.playback_job
                .as_ref()
                .filter(|job| job.request.generation == self.playback_generation)
                .map(|job| &job.request)
        })
    }
    pub fn preparing_playback(&self) -> bool {
        self.requested_playback().is_some()
    }
    pub(super) fn can_step_playback(&self, direction: i64) -> bool {
        let (length, index) = self
            .requested_playback()
            .map(|request| (request.queue.len(), request.index))
            .unwrap_or((self.queue.len(), self.queue_index));
        if direction < 0 {
            index > 0
        } else {
            index + 1 < length
        }
    }

    pub(super) fn play_selected(&mut self) -> Result<()> {
        let entry = self.entry().context("Select a track first")?;
        let playlist = self.playlist().context("Select a playlist first")?;
        let playlist_id = playlist.id;
        let queue: Arc<[Entry]> = playlist.entries.clone().into();
        let index = queue
            .iter()
            .position(|queued| queued.id == entry.id)
            .context("Track no longer exists")?;
        self.prepare_playback(queue, playlist_id, index)
    }
    fn prepare_playback(
        &mut self,
        queue: Arc<[Entry]>,
        playlist_id: i64,
        index: usize,
    ) -> Result<()> {
        ensure!(index < queue.len(), "Select a track first");
        let title = queue[index].track.title.clone();
        self.playback_generation = self.playback_generation.wrapping_add(1);
        self.cancel_waveform();
        self.pending_playback = Some(PlaybackRequest {
            queue,
            playlist_id,
            index,
            generation: self.playback_generation,
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
        if self.playback_job.is_none()
            && let Some(request) = self.pending_playback.take()
        {
            let path = request.queue[request.index].track.path.clone();
            self.playback_job = Some(PlaybackJob {
                receiver: background(self.runtime(), move || Audio::prepare(&path)),
                request,
            });
        }
    }
    pub(super) fn tick_playback(&mut self) -> Result<()> {
        let result = match self
            .playback_job
            .as_mut()
            .map(|job| job.receiver.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                Err(anyhow::anyhow!("Background operation was interrupted"))
            }
            _ => return Ok(()),
        };
        let job = self
            .playback_job
            .take()
            .expect("audio preparation is present");
        self.start_playback_job();
        if job.request.generation != self.playback_generation {
            return Ok(());
        }
        let source = match result {
            Ok(source) => source,
            Err(error) => {
                if !self.audio.as_ref().is_some_and(Audio::has_track) {
                    self.stop_playback();
                }
                return Err(error);
            }
        };
        let duration = source.total_duration();
        // Keep the platform audio device on its owning thread; only file access
        // and decoder preparation run in a blocking worker.
        if self.audio.is_none() {
            self.audio = Some(Audio::open(self.settings.volume)?);
        }
        let audio = self.audio.as_mut().context("Audio output unavailable")?;
        audio.play_prepared(source, job.request.paused);
        let entry = &job.request.queue[job.request.index];
        let mut track = entry.track.clone();
        if let Some(duration) = duration {
            track.duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        }
        self.playing_entry = Some(entry.id);
        self.playing_playlist = Some(job.request.playlist_id);
        self.queue = job.request.queue;
        self.queue_index = job.request.index;
        self.current = Some(track.clone());
        self.waveform = None;
        self.pending_waveform = Some((track.path.clone(), self.playback_generation));
        self.start_waveform_job();
        self.message(format!(
            "{}: {}",
            self.text("Playing", "Воспроизведение"),
            track.title
        ));
        Ok(())
    }
    pub(super) fn toggle_playback(&mut self) -> Result<()> {
        let generation = self.playback_generation;
        let request = self.pending_playback.as_mut().or_else(|| {
            self.playback_job
                .as_mut()
                .filter(|job| job.request.generation == generation)
                .map(|job| &mut job.request)
        });
        if let Some(request) = request {
            request.paused = !request.paused;
            if let Some(audio) = &self.audio {
                audio.set_paused(request.paused);
            }
        } else if self.current.is_some() {
            if let Some(audio) = &self.audio {
                audio.toggle();
            }
        } else {
            self.play_selected()?;
        }
        Ok(())
    }
    pub(super) fn stop_playback(&mut self) {
        self.playback_generation = self.playback_generation.wrapping_add(1);
        self.pending_playback = None;
        if let Some(audio) = &mut self.audio {
            audio.stop();
        }
        self.current = None;
        self.playing_entry = None;
        self.playing_playlist = None;
        self.waveform = None;
        self.cancel_waveform();
        self.queue = Arc::from([]);
        self.queue_index = 0;
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
                self.playing_playlist
                    .map(|id| (self.queue.clone(), id, self.queue_index))
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
        self.pending_waveform = None;
        if let Some(job) = &self.waveform_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn start_waveform_job(&mut self) {
        if self.waveform_job.is_none()
            && let Some((path, generation)) = self.pending_waveform.take()
        {
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            self.waveform_job = Some(WaveformJob {
                receiver: background(self.runtime(), move || {
                    Audio::waveform(&path, &worker_cancel)
                }),
                generation,
                cancel,
            });
        }
    }

    pub fn preparing_waveform(&self) -> bool {
        self.pending_waveform.is_some()
            || self.waveform_job.as_ref().is_some_and(|job| {
                job.generation == self.playback_generation && !job.cancel.load(Ordering::Relaxed)
            })
    }

    pub(super) fn tick_waveform(&mut self) {
        let result = match self
            .waveform_job
            .as_mut()
            .map(|job| job.receiver.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                Err(anyhow::anyhow!("Waveform worker was interrupted"))
            }
            _ => return,
        };
        let job = self.waveform_job.take().expect("waveform job exists");
        if job.generation == self.playback_generation
            && self.current.is_some()
            && !job.cancel.load(Ordering::Relaxed)
        {
            match result {
                Ok(waveform) => self.waveform = Some(waveform),
                Err(error) => {
                    self.waveform = Some(Vec::new());
                    self.failure(
                        error.context(self.text("Waveform unavailable", "Аудиоволна недоступна")),
                    );
                }
            }
        }
        self.start_waveform_job();
    }
}
