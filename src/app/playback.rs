use super::{
    App,
    background::{Background, background, poll},
};
use crate::errors::AppError;
use crate::{
    audio::Audio,
    model::{Entry, RepeatMode},
    preferences::PlaybackTimeline,
};
use anyhow::{Context, Result, ensure};
use rodio::Source;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{collections::VecDeque, path::PathBuf, time::Duration};

const SHUFFLE_HISTORY_LIMIT: usize = 256;

pub(super) struct PlaybackRequest {
    queue: Arc<[Entry]>,
    playlist_id: i64,
    index: usize,
    paused: bool,
    shuffle_history: VecDeque<usize>,
}

pub(super) struct SeekRequest {
    path: PathBuf,
    position_ms: u64,
}
pub(super) struct WaveformJob {
    receiver: Background<Vec<f32>>,
    generation: u64,
    pub(super) cancel: Arc<AtomicBool>,
}

impl App {
    pub(super) fn set_playback_timeline(&mut self, choice: PlaybackTimeline) -> Result<()> {
        if self.settings.appearance.playback_timeline != choice {
            self.cancel_workspace_drag();
            self.settings.appearance.playback_timeline = choice;
            self.save_settings()?;
        }
        Ok(())
    }

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
    pub(super) fn can_step_playback(&self, direction: i64) -> bool {
        let (length, index, history) = self
            .requested_playback()
            .map(|request| (request.queue.len(), request.index, &request.shuffle_history))
            .unwrap_or((
                self.playback.queue.len(),
                self.playback.queue_index,
                &self.playback.shuffle_history,
            ));
        if length == 0 || index >= length {
            return false;
        }
        if self.settings.shuffle {
            return if direction < 0 {
                !history.is_empty()
            } else {
                length > 1
            };
        }
        if self.settings.repeat == RepeatMode::Playlist {
            return true;
        }
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
    pub(super) fn prepare_playback(
        &mut self,
        queue: Arc<[Entry]>,
        playlist_id: i64,
        index: usize,
    ) -> Result<()> {
        self.request_playback(PlaybackRequest {
            queue,
            playlist_id,
            index,
            paused: false,
            shuffle_history: VecDeque::new(),
        })
    }

    fn request_playback(&mut self, request: PlaybackRequest) -> Result<()> {
        ensure!(
            request.index < request.queue.len(),
            AppError::TrackSelectionRequired
        );
        let title = request.queue[request.index].track.title.clone();
        self.cancel_seek();
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
        audio.equalizer(&self.settings.equalizer);
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
        self.playback.shuffle_history = request.shuffle_history;
        self.playback.current = Some(track.clone());
        self.playback.waveform = None;
        if self.settings.appearance.playback_timeline == PlaybackTimeline::Waveform {
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
        self.cancel_seek();
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
        self.clear_shuffle_history();
        self.message(
            self.text("Playback stopped", "Воспроизведение остановлено")
                .into(),
        );
    }

    pub(super) fn next(&mut self, direction: i64, explicit: bool) -> Result<()> {
        let Some((queue, playlist_id, index, mut history)) = self
            .requested_playback()
            .map(|request| {
                (
                    request.queue.clone(),
                    request.playlist_id,
                    request.index,
                    request.shuffle_history.clone(),
                )
            })
            .or_else(|| {
                self.playback.playing_playlist.map(|id| {
                    (
                        self.playback.queue.clone(),
                        id,
                        self.playback.queue_index,
                        self.playback.shuffle_history.clone(),
                    )
                })
            })
        else {
            return Ok(());
        };
        if queue.is_empty() || index >= queue.len() {
            if !explicit {
                self.stop_playback();
            }
            return Ok(());
        }
        // Repeating one track only intercepts completion, never a manual skip.
        let next = if !explicit && direction > 0 && self.settings.repeat == RepeatMode::Track {
            Some(index)
        } else if self.settings.shuffle {
            if direction < 0 {
                history.pop_back()
            } else if queue.len() > 1 {
                // Sample all other positions uniformly without retries. Earlier
                // tracks remain eligible, as only the current track is excluded.
                let choice = fastrand::usize(..queue.len() - 1);
                history.push_back(index);
                if history.len() > SHUFFLE_HISTORY_LIMIT {
                    history.pop_front();
                }
                Some(choice + usize::from(choice >= index))
            } else {
                None
            }
        } else {
            history.clear();
            if direction < 0 {
                index.checked_sub(1).or_else(|| {
                    (self.settings.repeat == RepeatMode::Playlist).then_some(queue.len() - 1)
                })
            } else if index + 1 < queue.len() {
                Some(index + 1)
            } else {
                (self.settings.repeat == RepeatMode::Playlist).then_some(0)
            }
        };
        if let Some(index) = next {
            self.request_playback(PlaybackRequest {
                queue,
                playlist_id,
                index,
                paused: false,
                shuffle_history: history,
            })
        } else {
            if !explicit {
                self.stop_playback();
            }
            Ok(())
        }
    }

    pub(super) fn clear_shuffle_history(&mut self) {
        self.playback.shuffle_history.clear();
        if let Some(request) = self.playback.preparation.requested_mut() {
            request.shuffle_history.clear();
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
            self.playback.seek.request(SeekRequest {
                path: track.path.clone(),
                position_ms,
            });
            self.start_seek_job();
        }
    }

    pub(super) fn seek_to(&mut self, position_ms: u64) {
        self.preview_seek(position_ms);
        self.commit_seek();
    }

    pub(super) fn cancel_seek(&mut self) {
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
        self.playback.seek.start(|request| {
            let path = request.path.clone();
            let position = Duration::from_millis(request.position_ms);
            background(runtime, move || Audio::prepare_at(&path, position))
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
                if let Some(audio) = &mut self.playback.audio {
                    audio.play_prepared_at(
                        source,
                        paused,
                        Duration::from_millis(completed.request.position_ms),
                    );
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
        if self.settings.appearance.playback_timeline == PlaybackTimeline::Progress {
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
    use crate::model::Track;
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
        app.playback.queue = (0..length)
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
        app.playback.queue_index = index;
        app.playback.playing_playlist = Some(42);
        app.playback.playing_entry = app.playback.queue.get(index).map(|entry| entry.id);
        app.playback.current = app
            .playback
            .queue
            .get(index)
            .map(|entry| entry.track.clone());
    }

    fn requested_index(app: &App) -> usize {
        app.requested_playback()
            .expect("Next track is being prepared")
            .index
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
        assert!(app.playback.queue.is_empty());
    }

    #[test]
    fn track_repeat_restarts_on_completion_but_does_not_trap_manual_skips() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 3, 1);
        app.settings.repeat = RepeatMode::Track;
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 1);
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 2);
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 2);
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 1);
    }

    #[test]
    fn playlist_repeat_wraps_in_both_directions_including_pending_requests() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 3, 2);
        app.settings.repeat = RepeatMode::Playlist;
        assert!(app.can_step_playback(1));
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 0);
        assert!(app.can_step_playback(-1));
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 2);
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 0);
        app.settings.repeat = RepeatMode::Off;
        assert!(!app.can_step_playback(-1));
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 0);
    }

    #[test]
    fn shuffle_excludes_only_the_current_track_and_previous_retraces_history() {
        fastrand::seed(7);
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 5, 4);
        app.settings.shuffle = true;
        assert!(app.can_step_playback(1));
        assert!(!app.can_step_playback(-1));
        let mut indices = vec![4];
        for _ in 0..128 {
            app.next(1, false).unwrap();
            let index = requested_index(&app);
            assert!(index < 5);
            assert_ne!(index, *indices.last().unwrap());
            assert_eq!(app.requested_playback().unwrap().playlist_id, 42);
            indices.push(index);
        }
        assert!((0..5).all(|index| indices.contains(&index)));
        for index in indices[..indices.len() - 1].iter().rev() {
            assert!(app.can_step_playback(-1));
            app.next(-1, true).unwrap();
            assert_eq!(requested_index(&app), *index);
        }
        assert!(!app.can_step_playback(-1));
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 4);
    }

    #[test]
    fn track_repeat_takes_priority_over_shuffle_and_skip_still_chooses_another_track() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 2, 1);
        app.settings.shuffle = true;
        app.settings.repeat = RepeatMode::Track;
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 1);
        app.next(1, true).unwrap();
        assert_eq!(requested_index(&app), 0);
        app.next(1, false).unwrap();
        assert_eq!(requested_index(&app), 0);
        app.next(-1, true).unwrap();
        assert_eq!(requested_index(&app), 1);
    }

    #[test]
    fn empty_and_single_track_playlists_handle_every_mode_without_invalid_requests() {
        let directory = Directory::new();
        for shuffle in [false, true] {
            for repeat in [RepeatMode::Off, RepeatMode::Playlist, RepeatMode::Track] {
                let mut app = App::new(&directory.0).unwrap();
                app.settings.shuffle = shuffle;
                app.settings.repeat = repeat;
                queue(&mut app, 0, 0);
                assert!(!app.can_step_playback(1));
                assert!(!app.can_step_playback(-1));
                app.next(1, false).unwrap();
                assert!(!app.preparing_playback());
                queue(&mut app, 1, 0);
                app.next(1, false).unwrap();
                if repeat == RepeatMode::Track || (!shuffle && repeat == RepeatMode::Playlist) {
                    assert_eq!(requested_index(&app), 0);
                } else {
                    assert!(app.playback.current.is_none());
                    assert!(!app.preparing_playback());
                }
            }
        }
    }

    #[test]
    fn shuffle_history_is_bounded_and_resets_on_explicit_selection_and_stop() {
        let directory = Directory::new();
        let mut app = App::new(&directory.0).unwrap();
        queue(&mut app, 2, 0);
        app.settings.shuffle = true;
        for _ in 0..SHUFFLE_HISTORY_LIMIT + 10 {
            app.next(1, true).unwrap();
        }
        assert_eq!(
            app.requested_playback().unwrap().shuffle_history.len(),
            SHUFFLE_HISTORY_LIMIT
        );
        assert!(app.can_step_playback(-1));
        app.prepare_playback(app.playback.queue.clone(), 99, 0)
            .unwrap();
        assert!(!app.can_step_playback(-1));
        assert_eq!(app.requested_playback().unwrap().playlist_id, 99);
        app.next(1, true).unwrap();
        assert!(app.can_step_playback(-1));
        app.stop_playback();
        assert!(!app.can_step_playback(-1));
        assert!(!app.can_step_playback(1));
        assert!(app.playback.shuffle_history.is_empty());
    }
}
