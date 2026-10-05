use super::{App, bounded};
use crate::{audio::Audio, model::Track};
use anyhow::{Context, Result};

impl App {
    pub fn position_ms(&self) -> u64 {
        if self.current.is_none() {
            return 0;
        }
        self.audio.as_ref().map_or(0, |audio| {
            audio.position().as_millis().min(u64::MAX as u128) as u64
        })
    }

    pub fn paused(&self) -> bool {
        self.audio.as_ref().is_none_or(Audio::paused) || self.current.is_none()
    }

    pub(super) fn play_selected(&mut self) -> Result<()> {
        let entry = self.entry().context("Select a track first")?;
        let queue: Vec<Track> = self
            .playlist()
            .context("Select a playlist first")?
            .entries
            .iter()
            .map(|entry| entry.track.clone())
            .collect();
        let index = queue
            .iter()
            .position(|track| track.id == entry.track.id)
            .context("Track no longer exists")?;
        self.play_track(&queue[index])?;
        self.queue = queue;
        self.queue_index = index;
        Ok(())
    }

    fn play_track(&mut self, track: &Track) -> Result<()> {
        if self.audio.is_none() {
            self.audio = Some(Audio::open(self.settings.volume)?);
        }
        self.audio
            .as_mut()
            .context("Audio output unavailable")?
            .play(&track.path)?;
        self.current = Some(track.clone());
        self.message(format!(
            "{}: {}",
            self.text("Playing", "Воспроизведение"),
            track.title
        ));
        Ok(())
    }

    pub(super) fn next(&mut self, direction: i64, explicit: bool) -> Result<()> {
        if direction > 0 && self.queue_index + 1 >= self.queue.len() {
            if explicit {
                return Ok(());
            }
            self.current = None;
            self.queue.clear();
            return Ok(());
        }
        if self.queue.is_empty() {
            return Ok(());
        }
        let index = bounded(self.queue_index, direction, self.queue.len());
        let track = self.queue[index].clone();
        self.play_track(&track)?;
        self.queue_index = index;
        Ok(())
    }
}
