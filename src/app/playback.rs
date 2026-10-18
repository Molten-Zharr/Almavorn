use super::{
    App,
    background::{Background, background, poll},
    playback_queue::{PlaybackQueue, QueueStep, QueueUpdate},
};
use crate::errors::AppError;
use crate::{audio::Audio, model::Entry, preferences::PlaybackTimeline};
use anyhow::{Context, Result, ensure};
use rodio::Source;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::HashSet, path::PathBuf, time::Duration};

#[derive(Clone)]
pub(super) struct PlaybackRequest {
    queue: PlaybackQueue,
    playlist_id: i64,
    paused: bool,
    reverse: bool,
}

impl PlaybackRequest {
    fn entry(&self) -> &Entry {
        self.queue.current().expect("playback request has an entry")
    }
}

pub(super) struct SeekRequest {
    path: PathBuf,
    position_ms: u64,
    pub(super) reverse: bool,
}
pub(super) struct WaveformJob {
    receiver: Background<Vec<f32>>,
    generation: u64,
    pub(super) cancel: Arc<AtomicBool>,
}

impl App {
    pub(super) fn set_playback_timeline(&mut self, choice: PlaybackTimeline) -> Result<()> {
        if self.settings.preferences.appearance.playback_timeline != choice {
            self.cancel_workspace_drag();
            self.settings.preferences.appearance.playback_timeline = choice;
            self.save_settings()?;
        }
        Ok(())
    }

    pub(super) fn adjust_volume(&mut self, step: i16) -> Result<()> {
        let percent = (self.settings.preferences.volume * 100.0).round() as i16;
        self.set_volume((percent + step).clamp(0, 100) as f32 / 100.0)
    }

    pub(super) fn set_volume(&mut self, volume: f32) -> Result<()> {
        let volume = volume.clamp(0.0, 1.0);
        if self.settings.preferences.volume == volume {
            return Ok(());
        }
        self.settings.preferences.volume = volume;
        self.sync_volume();
        self.save_settings()
    }

    pub(super) fn sync_volume(&self) {
        if let Some(audio) = &self.playback.audio {
            audio.volume(self.settings.preferences.volume);
        }
    }

    pub fn position_ms(&self) -> u64 {
        if let Some(position) = self.playback.seek_preview {
            return position;
        }
        if self.playback.current.is_none() {
            return 0;
        }
        self.playback.audio.as_ref().map_or(0, |audio| {
            audio.position().as_millis().min(u64::MAX as u128) as u64
        })
    }

    pub fn paused(&self) -> bool {
        if let Some(paused) = self.playback.seek_paused {
            return paused;
        }
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
        self.playback
            .seek_paused
            .unwrap_or_else(|| self.playback.audio.as_ref().is_none_or(Audio::paused))
    }
    fn requested_playback(&self) -> Option<&PlaybackRequest> {
        self.playback.preparation.requested()
    }
    pub fn preparing_playback(&self) -> bool {
        self.requested_playback().is_some()
    }
    fn marked_queue(&self) -> Option<(i64, &[Entry])> {
        if let Some(request) = self.requested_playback() {
            return request
                .queue
                .marked_only()
                .then_some((request.playlist_id, request.queue.entries()));
        }
        self.playback.queue.marked_only().then_some(())?;
        Some((
            self.playback.playing_playlist?,
            self.playback.queue.entries(),
        ))
    }
    pub fn marked_playback_count(&self) -> Option<usize> {
        self.marked_queue().map(|(_, queue)| queue.len())
    }
    pub(super) fn marked_playback_playlist(&self) -> Option<i64> {
        self.marked_queue().map(|(id, _)| id)
    }
    pub(super) fn restore_playback_marks(&mut self) {
        if let Some((id, queue)) = self.marked_queue()
            && self.library.selected_playlist == Some(id)
        {
            self.library.marked = queue.iter().map(|entry| entry.id).collect();
        }
    }
    fn playback_request(&self) -> Option<PlaybackRequest> {
        self.requested_playback().cloned().or_else(|| {
            Some(PlaybackRequest {
                queue: self.playback.queue.clone(),
                playlist_id: self.playback.playing_playlist?,
                paused: self.paused(),
                reverse: false,
            })
        })
    }
    pub(super) fn play_marked(&mut self) -> Result<()> {
        let playlist = self
            .playlist()
            .context(AppError::PlaylistSelectionRequired)?;
        let playlist_id = playlist.id;
        let queue: Arc<[Entry]> = playlist
            .entries
            .iter()
            .filter(|entry| self.library.marked.contains(&entry.id))
            .cloned()
            .collect();
        ensure!(!queue.is_empty(), AppError::TrackSelectionRequired);
        let index = queue
            .iter()
            .position(|entry| Some(entry.id) == self.library.selected_entry)
            .unwrap_or(0);
        self.request_playback(PlaybackRequest {
            queue: PlaybackQueue::new(queue, index, true),
            playlist_id,
            paused: false,
            reverse: false,
        })
    }
    pub(super) fn toggle_mark(&mut self, id: i64) -> Result<()> {
        if !self.library.marked.insert(id) {
            self.library.marked.remove(&id);
        }
        self.sync_marked_playback()
    }
    pub(super) fn play_whole_playlist(&mut self) -> Result<()> {
        let Some(request) = self
            .playback_request()
            .filter(|request| request.queue.marked_only())
        else {
            return Ok(());
        };
        let Some(playlist) = self
            .library
            .playlists
            .iter()
            .find(|playlist| playlist.id == request.playlist_id)
        else {
            self.stop_playback();
            return Ok(());
        };
        let queue = playlist.entries.clone().into();
        self.update_playback_queue(request, queue, false)?;
        self.message(
            self.text(
                "Playing the whole playlist",
                "Воспроизведение всего плейлиста",
            )
            .into(),
        );
        Ok(())
    }
    pub(super) fn sync_marked_playback(&mut self) -> Result<()> {
        let Some(request) = self
            .playback_request()
            .filter(|request| request.queue.marked_only())
        else {
            return Ok(());
        };
        let Some(playlist) = self
            .library
            .playlists
            .iter()
            .find(|playlist| playlist.id == request.playlist_id)
        else {
            self.stop_playback();
            return Ok(());
        };
        // Marks in another playlist belong to that view, not to the playing set.
        let active: HashSet<_> = request
            .queue
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect();
        let queue = playlist
            .entries
            .iter()
            .filter(|entry| {
                if self.library.selected_playlist == Some(playlist.id) {
                    self.library.marked.contains(&entry.id)
                } else {
                    active.contains(&entry.id)
                }
            })
            .cloned()
            .collect();
        self.update_playback_queue(request, queue, true)
    }
    fn update_playback_queue(
        &mut self,
        mut request: PlaybackRequest,
        queue: Arc<[Entry]>,
        marked_only: bool,
    ) -> Result<()> {
        let update = request.queue.replace(queue.clone(), marked_only);
        if update == QueueUpdate::Empty {
            self.stop_playback();
            return Ok(());
        }
        let current_index = self
            .playback
            .playing_entry
            .filter(|_| self.playback.playing_playlist == Some(request.playlist_id))
            .and_then(|id| queue.iter().position(|entry| entry.id == id));
        if let Some(index) = current_index {
            self.playback.queue.retarget(queue, index, marked_only);
        }
        if update == QueueUpdate::Kept {
            if let Some(pending) = self.playback.preparation.requested_mut() {
                // The prepared source is still for the same entry and path.
                request.reverse = pending.reverse;
                *pending = request;
            }
        } else {
            self.request_playback(request)?;
        }
        if current_index.is_none()
            && let Some(audio) = &self.playback.audio
        {
            // Never continue playing a track removed from the active set.
            audio.set_paused(true);
        }
        Ok(())
    }
    pub(super) fn can_step_playback(&self, direction: i64) -> bool {
        let queue = self
            .requested_playback()
            .map_or(&self.playback.queue, |request| &request.queue);
        queue.can_step(
            QueueStep::manual(direction),
            self.settings.preferences.repeat,
            self.settings.preferences.shuffle,
        )
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
    pub(super) fn prepare_playback(
        &mut self,
        queue: Arc<[Entry]>,
        playlist_id: i64,
        index: usize,
    ) -> Result<()> {
        self.request_playback(PlaybackRequest {
            queue: PlaybackQueue::new(queue, index, false),
            playlist_id,
            paused: false,
            reverse: false,
        })
    }

    fn request_playback(&mut self, mut request: PlaybackRequest) -> Result<()> {
        ensure!(
            request.queue.current().is_some(),
            AppError::TrackSelectionRequired
        );
        let title = request.entry().track.title.clone();
        request.reverse = self.tuner_for_path(&request.entry().track.path).reverse;
        self.cancel_seek();
        self.cancel_audio_preparation();
        self.cancel_waveform();
        self.playback.preparation.request(request);
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
        let cached = self.playback.reverse_cache.clone();
        self.playback.preparation.start(|request| {
            let reverse = request.reverse;
            let path = request.entry().track.path.clone();
            let cache = cached
                .filter(|(cached_path, _)| *cached_path == path)
                .map(|(_, cache)| cache);
            let cancel = Arc::new(AtomicBool::new(false));
            self.playback.preparation_cancel = Some(cancel.clone());
            background(runtime, move || {
                Audio::prepare_tuned(&path, None, reverse, cache, &cancel)
            })
        });
    }
    pub(super) fn cancel_audio_preparation(&mut self) {
        if let Some(cancel) = self.playback.preparation_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
    pub(super) fn sync_playback_direction(&mut self) -> Result<()> {
        if let Some(request) = self.requested_playback()
            && request.reverse != self.tuner_for_path(&request.entry().track.path).reverse
        {
            self.request_playback(request.clone())?;
        }
        Ok(())
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
                if !self.playback.audio.as_ref().is_some_and(Audio::has_track)
                    || (request.queue.marked_only()
                        && !request
                            .queue
                            .entries()
                            .iter()
                            .any(|entry| Some(entry.id) == self.playback.playing_entry))
                {
                    self.stop_playback();
                }
                return Err(error);
            }
        };
        let path = request.entry().track.path.clone();
        if let Some(cache) = &source.cache {
            self.playback.reverse_cache = Some((path.clone(), cache.clone()));
        } else if self
            .playback
            .reverse_cache
            .as_ref()
            .is_some_and(|(cached, _)| *cached != path)
        {
            self.playback.reverse_cache = None;
        }
        let tuner = self.tuner_for_path(&path).clone();
        if source.reversed() != tuner.reverse {
            return self.request_playback(request);
        }
        let duration = source.total_duration();
        // Keep the platform audio device on its owning thread; only file access
        // and decoder preparation run in a blocking worker.
        if self.playback.audio.is_none() {
            self.playback.audio = Some(Audio::open(self.settings.preferences.volume)?);
        }
        self.finish_listening();
        let audio = self
            .playback
            .audio
            .as_mut()
            .context(AppError::AudioUnavailable)?;
        audio.equalizer(&self.settings.preferences.equalizer);
        audio.tuner(&tuner);
        audio.play_prepared(source, request.paused);
        let entry = request.entry();
        let mut track = entry.track.clone();
        if let Some(duration) = duration {
            track.duration_ms = duration.as_millis().min(u128::from(u64::MAX)) as u64;
        }
        self.begin_listening(track.id, track.duration_ms);
        self.playback.playing_entry = Some(entry.id);
        self.playback.playing_playlist = Some(request.playlist_id);
        self.playback.queue = request.queue;
        self.playback.current = Some(track.clone());
        self.playback.waveform = None;
        if self.settings.preferences.appearance.playback_timeline == PlaybackTimeline::Waveform {
            self.playback.pending_waveform =
                Some((track.path.clone(), self.playback.preparation.generation()));
            self.start_waveform_job();
        }
        self.message(format!(
            "{}: {}",
            self.text("Playing", "Воспроизведение"),
            track.title
        ));
        Ok(())
    }
    pub(super) fn toggle_playback(&mut self) -> Result<()> {
        if let Some(paused) = &mut self.playback.seek_paused {
            *paused = !*paused;
            return Ok(());
        }
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
        self.finish_listening();
        self.playback.visualizer.reset();
        self.cancel_seek();
        self.cancel_audio_preparation();
        self.playback.preparation.cancel();
        self.playback.reverse_cache = None;
        if let Some(audio) = &mut self.playback.audio {
            audio.stop();
        }
        self.playback.current = None;
        self.playback.playing_entry = None;
        self.playback.playing_playlist = None;
        self.playback.waveform = None;
        self.cancel_waveform();
        self.playback.queue = PlaybackQueue::default();
        self.message(
            self.text("Playback stopped", "Воспроизведение остановлено")
                .into(),
        );
    }

    pub(super) fn next(&mut self, direction: i64, explicit: bool) -> Result<()> {
        let Some(mut request) = self.playback_request() else {
            return Ok(());
        };
        let step = if !explicit && direction > 0 {
            QueueStep::Finished
        } else {
            QueueStep::manual(direction)
        };
        if let Some(queue) = request.queue.advance(
            step,
            self.settings.preferences.repeat,
            self.settings.preferences.shuffle,
        ) {
            request.queue = queue;
            request.paused = false;
            self.request_playback(request)
        } else {
            if !explicit {
                self.stop_playback();
            }
            Ok(())
        }
    }

    pub(super) fn clear_shuffle_history(&mut self) {
        self.playback.queue.clear_history();
        if let Some(request) = self.playback.preparation.requested_mut() {
            request.queue.clear_history();
        }
    }

    pub(super) fn cancel_waveform(&mut self) {
        self.playback.pending_waveform = None;
        if let Some(job) = &self.playback.waveform_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn preview_seek(&mut self, position_ms: u64) {
        let Some(track) = &self.playback.current else {
            return;
        };
        let position_ms = position_ms.min(track.duration_ms.saturating_sub(1));
        if self.playback.seek_paused.is_none() {
            self.playback.seek_paused = Some(self.current_paused());
            if let Some(audio) = &self.playback.audio {
                audio.set_paused(true);
            }
        }
        self.playback.seek_preview = Some(position_ms);
    }

    pub(super) fn commit_seek(&mut self) {
        if let (Some(position_ms), Some(track)) =
            (self.playback.seek_preview, &self.playback.current)
        {
            if let Some(cancel) = self.playback.seek_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.playback.seek.request(SeekRequest {
                path: track.path.clone(),
                position_ms,
                reverse: self.tuner_for_path(&track.path).reverse,
            });
            self.start_seek_job();
        }
    }

    pub(super) fn seek_to(&mut self, position_ms: u64) {
        self.preview_seek(position_ms);
        self.commit_seek();
    }

    pub(super) fn cancel_seek(&mut self) {
        if let Some(cancel) = self.playback.seek_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.playback.seek.cancel();
        self.playback.seek_preview = None;
        if let Some(paused) = self.playback.seek_paused.take()
            && let Some(audio) = &self.playback.audio
        {
            audio.set_paused(paused);
        }
        if matches!(
            self.view.workspace.gesture,
            Some(crate::workspace::Gesture::Seek(_))
        ) {
            self.view.workspace.gesture = None;
        }
    }

    fn start_seek_job(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        let cached = self.playback.reverse_cache.clone();
        self.playback.seek.start(|request| {
            let path = request.path.clone();
            let position = Duration::from_millis(request.position_ms);
            let reverse = request.reverse;
            let cache = cached
                .filter(|(cached_path, _)| *cached_path == path)
                .map(|(_, cache)| cache);
            let cancel = Arc::new(AtomicBool::new(false));
            self.playback.seek_cancel = Some(cancel.clone());
            background(runtime, move || {
                Audio::prepare_tuned(&path, Some(position), reverse, cache, &cancel)
            })
        });
    }

    pub(super) fn tick_seek(&mut self) -> Result<()> {
        let Some(completed) = self.playback.seek.poll() else {
            return Ok(());
        };
        self.start_seek_job();
        if !completed.current {
            return Ok(());
        }
        if completed.request.reverse != self.tuner_for_path(&completed.request.path).reverse {
            self.seek_to(completed.request.position_ms);
            return Ok(());
        }
        // A new drag may already be previewing another position. Finish this
        // decode without replacing the audible source until that drag ends.
        if matches!(
            self.view.workspace.gesture,
            Some(crate::workspace::Gesture::Seek(_))
        ) {
            return Ok(());
        }
        let paused = self.playback.seek_paused.take().unwrap_or(true);
        self.playback.seek_preview = None;
        match completed.result {
            Ok(source) => {
                if let Some(cache) = &source.cache {
                    self.playback.reverse_cache =
                        Some((completed.request.path.clone(), cache.clone()));
                }
                if let Some(audio) = &mut self.playback.audio {
                    let changed_direction = audio.reversed() != source.reversed();
                    audio.play_prepared_at(
                        source,
                        paused,
                        Duration::from_millis(completed.request.position_ms),
                    );
                    if changed_direction {
                        self.message(
                            if completed.request.reverse {
                                self.text("Direction: reverse", "Направление: обратное")
                            } else {
                                self.text("Direction: forward", "Направление: прямое")
                            }
                            .into(),
                        );
                    }
                }
            }
            Err(error) => {
                if let Some(audio) = &self.playback.audio {
                    audio.set_paused(paused);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// GUI pointer positions retain pixel precision; terminal events use cells.
    pub fn scrub(&mut self, ratio: f64, release: bool) {
        if !matches!(
            self.view.workspace.gesture,
            Some(crate::workspace::Gesture::Seek(_))
        ) || !ratio.is_finite()
        {
            return;
        }
        if let Some(track) = &self.playback.current {
            self.preview_seek((ratio.clamp(0.0, 1.0) * track.duration_ms as f64).round() as u64);
        }
        if release {
            self.view.workspace.gesture = None;
            self.commit_seek();
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
        if self.settings.preferences.appearance.playback_timeline == PlaybackTimeline::Progress {
            self.cancel_waveform();
        } else if self.playback.waveform.is_none()
            && !self.preparing_playback()
            && !self.preparing_waveform()
            && let Some(track) = &self.playback.current
        {
            self.playback.pending_waveform =
                Some((track.path.clone(), self.playback.preparation.generation()));
            self.start_waveform_job();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RepeatMode, Track};
    use std::{fs, sync::atomic::AtomicU64};

    struct Directory(PathBuf);

    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "almavorn-playback-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn queue(app: &mut App, length: usize, index: usize) {
        let entries = (0..length)
            .map(|index| Entry {
                id: index as i64 + 1,
                position: index as i64,
                track: Track {
                    id: index as i64 + 1,
                    path: app
                        .store
                        .path
                        .with_file_name(format!("missing-{index}.wav")),
                    title: format!("Track {index}"),
                    artist: String::new(),
                    album: String::new(),
                    duration_ms: 1000,
                    tags: "{}".into(),
                },
            })
            .collect::<Vec<_>>()
            .into();
        app.playback.queue = PlaybackQueue::new(entries, index, false);
        app.playback.playing_playlist = Some(42);
        app.playback.playing_entry = app.playback.queue.current().map(|entry| entry.id);
        app.playback.current = app
            .playback
            .queue
            .current()
            .map(|entry| entry.track.clone());
    }

    fn requested_index(app: &App) -> usize {
        let request = app
            .requested_playback()
            .expect("Next track is being prepared");
        request
            .queue
            .entries()
            .iter()
            .position(|entry| entry.id == request.entry().id)
            .unwrap()
    }

    fn library_queue(app: &mut App, length: usize, index: usize) {
        queue(app, length, index);
        app.library.playlists = vec![crate::model::Playlist {
            id: 42,
            name: "Playback test".into(),
            mode: crate::model::Mode::Chaos,
            kind: crate::model::PlaylistKind::Normal,
            position: 0,
            entries: app.playback.queue.entries().to_vec(),
            folders: Vec::new(),
            group_folders: false,
        }];
        app.library.selected_playlist = Some(42);
        app.library.selected_entry = app.playback.playing_entry;
        app.library.marked = app
            .playback
            .queue
            .entries()
            .iter()
            .map(|entry| entry.id)
            .collect();
    }

    #[test]
    fn marked_playback_wraps_the_set_and_repeats_a_single_mark_even_with_shuffle() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        library_queue(&mut app, 3, 1);
        app.settings.preferences.repeat = RepeatMode::Track;
        app.play_marked().unwrap();
        for expected in [2, 0, 1] {
            assert!(app.can_step_playback(1));
            app.next(1, false).unwrap();
            assert_eq!(requested_index(&app), expected);
        }
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 0);

        app.stop_playback();
        app.library.marked = HashSet::from([2]);
        app.settings.preferences.shuffle = true;
        app.settings.preferences.repeat = RepeatMode::Off;
        app.play_marked().unwrap();
        for _ in 0..3 {
            assert!(app.can_step_playback(1));
            app.next(1, false).unwrap();
            assert_eq!(requested_index(&app), 0);
            assert_eq!(app.requested_playback().unwrap().entry().id, 2);
        }
    }

    #[test]
    fn removing_the_playing_mark_selects_its_successor_and_empty_marks_stop() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        library_queue(&mut app, 4, 1);
        let entries = app.playback.queue.entries().to_vec().into();
        app.playback.queue = PlaybackQueue::new(entries, 1, true);
        app.toggle_mark(2).unwrap();
        let request = app.requested_playback().unwrap();
        assert_eq!(request.entry().id, 3);
        assert!(request.queue.marked_only());

        for id in [1, 3, 4] {
            app.toggle_mark(id).unwrap();
        }
        assert!(!app.preparing_playback());
        assert!(app.playback.current.is_none());
        assert!(app.playback.queue.entries().is_empty());
    }

    #[test]
    fn changing_marks_keeps_a_pending_decode_for_the_same_entry_and_preserves_pause() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        library_queue(&mut app, 4, 0);
        app.library.selected_entry = Some(2);
        app.play_marked().unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        let generation = app.playback.preparation.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();

        app.toggle_mark(1).unwrap();
        assert_eq!(app.playback.preparation.generation(), generation);
        assert!(!cancel.load(Ordering::Relaxed));
        let request = app.requested_playback().unwrap();
        assert_eq!(requested_index(&app), 0);
        assert_eq!(request.entry().id, 2);
        assert_eq!(
            request
                .queue
                .entries()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            [2, 3, 4]
        );
        assert!(request.paused);
    }

    #[test]
    fn removing_a_pending_mark_cancels_its_decode_and_retargets_the_latest_queue() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        library_queue(&mut app, 4, 0);
        app.library.selected_entry = Some(2);
        app.play_marked().unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        let generation = app.playback.preparation.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();

        app.toggle_mark(2).unwrap();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(app.playback.preparation.generation() > generation);
        let request = app.requested_playback().unwrap();
        assert_eq!(request.entry().id, 3);
        assert_eq!(
            request
                .queue
                .entries()
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            [1, 3, 4]
        );
        assert!(request.paused);
        assert_eq!(app.playback.current.as_ref().unwrap().id, 1);
        assert_eq!(app.playback.queue.current().unwrap().id, 1);
    }

    #[test]
    fn normal_completion_stops_at_the_end_and_manual_steps_use_the_playing_queue() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 3, 0);
        app.library.selected_playlist = Some(99);
        assert!(!app.can_step_playback(-1));
        assert!(app.can_step_playback(1));
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 1);
        assert_eq!(app.requested_playback().unwrap().playlist_id, 42);
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 2);
        assert!(!app.can_step_playback(1));
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 2);
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 1);
        app.next(1, false).unwrap();
        app.next(1, false).unwrap();
        assert!(!app.preparing_playback());
        assert!(app.playback.current.is_none());
        assert!(app.playback.playing_playlist.is_none());
        assert!(app.playback.queue.entries().is_empty());
    }

    #[test]
    fn whole_playlist_restores_normal_navigation_without_restarting_a_kept_decode() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        library_queue(&mut app, 4, 0);
        app.library.marked = HashSet::from([2, 4]);
        app.library.selected_entry = Some(2);
        app.play_marked().unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        let generation = app.playback.preparation.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();

        app.play_whole_playlist().unwrap();
        assert_eq!(app.playback.preparation.generation(), generation);
        assert!(!cancel.load(Ordering::Relaxed));
        let request = app.requested_playback().unwrap();
        assert_eq!(request.entry().id, 2);
        assert!(request.paused);
        assert_eq!(request.queue.entries().len(), 4);
        assert!(app.marked_playback_count().is_none());
        app.next(-1, true).unwrap();
        assert_eq!(app.requested_playback().unwrap().entry().id, 1);
        assert!(!app.can_step_playback(-1));
    }

    #[test]
    fn shuffle_history_resets_on_explicit_selection_and_stop() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 2, 0);
        app.settings.preferences.shuffle = true;
        app.next(1, true).unwrap();
        assert!(app.can_step_playback(-1));
        app.prepare_playback(app.playback.queue.entries().to_vec().into(), 99, 0)
            .unwrap();
        assert!(!app.can_step_playback(-1));
        assert_eq!(app.requested_playback().unwrap().playlist_id, 99);
        app.next(1, true).unwrap();
        assert!(app.can_step_playback(-1));
        app.stop_playback();
        assert!(!app.can_step_playback(-1));
        assert!(!app.can_step_playback(1));
        assert!(app.playback.queue.current().is_none());
    }

    #[test]
    fn pinned_reverse_uses_the_destination_file_for_skips_repeat_and_pending_changes() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 3, 0);
        app.settings.preferences.tuner.reverse = true;
        let pinned_path = app.playback.queue.entries()[1].track.path.clone();
        app.settings
            .track_tuners
            .insert(pinned_path, Default::default());
        assert!(app.tuner_settings().reverse);

        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 1);
        assert!(!app.requested_playback().unwrap().reverse);
        let generation = app.playback.preparation.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();
        // Changes to global FX cannot replace a pinned destination's decode.
        app.settings.preferences.tuner.reverse = false;
        app.sync_tuner().unwrap();
        app.settings.preferences.tuner.reverse = true;
        app.sync_tuner().unwrap();
        assert_eq!(app.playback.preparation.generation(), generation);
        assert!(!cancel.load(Ordering::Relaxed));

        app.settings.preferences.repeat = RepeatMode::Track;
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 1);
        assert!(!app.requested_playback().unwrap().reverse);
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 2);
        assert!(app.requested_playback().unwrap().reverse);
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 1);
        assert!(!app.requested_playback().unwrap().reverse);
    }

    #[test]
    fn editing_current_pinned_reverse_leaves_another_pending_track_alone_and_seeks_use_its_fx() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 2, 0);
        app.settings.preferences.tuner.reverse = true;
        let path = app.playback.current.as_ref().unwrap().path.clone();
        app.settings
            .track_tuners
            .insert(path.clone(), Default::default());
        app.prepare_playback(app.playback.queue.entries().to_vec().into(), 42, 1)
            .unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        let generation = app.playback.preparation.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();
        app.tuner_target(super::super::Target::TunerReverse)
            .unwrap();
        assert!(app.tuner_settings().reverse);
        assert_eq!(app.playback.preparation.generation(), generation);
        assert!(!cancel.load(Ordering::Relaxed));
        assert!(app.requested_playback().unwrap().paused);

        app.settings.preferences.tuner.reverse = false;
        app.sync_tuner().unwrap();
        assert!(cancel.load(Ordering::Relaxed));
        let request = app.requested_playback().unwrap();
        assert!(!request.reverse);
        assert!(request.paused);
        assert_eq!(requested_index(&app), 1);
        assert_eq!(request.playlist_id, 42);
        app.cancel_audio_preparation();
        app.playback.preparation.cancel();
        app.seek_to(500);
        assert!(app.playback.seek.requested().unwrap().reverse);
        app.cancel_seek();
        app.settings.track_tuners.get_mut(&path).unwrap().reverse = false;
        app.settings.preferences.tuner.reverse = true;
        app.seek_to(300);
        let seek = app.playback.seek.requested().unwrap();
        assert!(!seek.reverse);
        assert_eq!(seek.path, path);
        assert_eq!(seek.position_ms, 300);
    }

    #[test]
    fn changing_reverse_while_preparing_cancels_old_work_and_preserves_the_latest_queue_and_pause()
    {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 3, 1);
        app.prepare_playback(app.playback.queue.entries().to_vec().into(), 42, 1)
            .unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        let cancel = app.playback.preparation_cancel.clone().unwrap();
        let generation = app.playback.preparation.generation();
        app.settings.preferences.tuner.reverse = true;
        app.sync_tuner().unwrap();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(app.playback.preparation.generation() > generation);
        let request = app.requested_playback().unwrap();
        assert!(request.reverse);
        assert!(request.paused);
        assert_eq!(requested_index(&app), 1);
        assert_eq!(request.playlist_id, 42);
        app.settings.preferences.tuner.reverse = false;
        app.sync_tuner().unwrap();
        assert!(!app.requested_playback().unwrap().reverse);
        assert!(app.requested_playback().unwrap().paused);
        app.stop_playback();
        assert!(!app.preparing_playback());
        assert!(app.playback.reverse_cache.is_none());
    }

    #[test]
    fn saving_settings_and_editing_appearance_volume_or_eq_leave_decode_and_seek_alone() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 2, 0);
        app.next(1, true).unwrap();
        app.playback.preparation.requested_mut().unwrap().paused = true;
        app.seek_to(400);
        let preparation = app.playback.preparation.generation();
        let seek = app.playback.seek.generation();
        let cancel = app.playback.preparation_cancel.clone().unwrap();
        let seek_cancel = app.playback.seek_cancel.clone().unwrap();
        // Persistence must not apply even an unapplied direction change.
        app.settings.preferences.tuner.reverse = true;
        app.save_settings().unwrap();
        app.open_settings_page(super::super::SettingsPage::Typography);
        app.adjust_setting(1, 1).unwrap();
        app.adjust_volume(5).unwrap();
        app.equalizer_target(super::super::Target::EqualizerToggle)
            .unwrap();

        assert_eq!(app.playback.preparation.generation(), preparation);
        assert_eq!(app.playback.seek.generation(), seek);
        assert!(!cancel.load(Ordering::Relaxed));
        assert!(!seek_cancel.load(Ordering::Relaxed));
        assert!(app.requested_playback().unwrap().paused);
        assert!(!app.requested_playback().unwrap().reverse);
        assert_eq!(app.playback.seek_preview, Some(400));
        assert!(app.settings_saving());

        // FX editing still applies the direction immediately and preserves pause.
        app.tuner_target(super::super::Target::TunerReset).unwrap();
        app.tuner_target(super::super::Target::TunerReverse)
            .unwrap();
        assert!(cancel.load(Ordering::Relaxed));
        assert!(seek_cancel.load(Ordering::Relaxed));
        assert!(app.requested_playback().unwrap().reverse);
        assert!(app.requested_playback().unwrap().paused);
    }

    #[test]
    fn applying_a_profile_updates_pending_direction_but_preserves_a_pinned_destination() {
        for pinned in [false, true] {
            let directory = Directory::new();
            let mut app = App::new(&directory.0).unwrap();
            queue(&mut app, 2, 0);
            if pinned {
                app.settings.track_tuners.insert(
                    app.playback.queue.entries()[1].track.path.clone(),
                    Default::default(),
                );
            }
            app.next(1, true).unwrap();
            app.playback.preparation.requested_mut().unwrap().paused = true;
            let generation = app.playback.preparation.generation();
            let cancel = app.playback.preparation_cancel.clone().unwrap();
            let mut preferences = app.settings.profile_preferences();
            preferences.volume = 0.25;
            preferences.equalizer.enabled = true;
            preferences.tuner.reverse = true;
            app.settings
                .profiles
                .push(crate::preferences::SettingsProfile {
                    id: "reverse".into(),
                    name: "Reverse".into(),
                    preferences,
                });
            app.open_settings_page(super::super::SettingsPage::Profiles);
            app.adjust_setting(0, 1).unwrap();
            app.setting(1).unwrap();
            assert_eq!(app.settings.active_profile, "reverse");
            assert_eq!(app.settings.preferences.volume, 0.25);
            assert!(app.settings.preferences.equalizer.enabled);
            assert!(app.settings.preferences.tuner.reverse);
            assert_eq!(app.settings.track_tuners.len(), usize::from(pinned));
            assert_eq!(app.requested_playback().unwrap().reverse, !pinned);
            assert!(app.requested_playback().unwrap().paused);
            assert_eq!(requested_index(&app), 1);
            if pinned {
                assert_eq!(app.playback.preparation.generation(), generation);
                assert!(!cancel.load(Ordering::Relaxed));
            } else {
                assert!(app.playback.preparation.generation() > generation);
                assert!(cancel.load(Ordering::Relaxed));
            }
        }
    }
}
