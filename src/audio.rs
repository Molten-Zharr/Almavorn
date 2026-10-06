use crate::errors::AppError;
use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub type PreparedAudio = Decoder<BufReader<File>>;

pub struct Audio {
    device: MixerDeviceSink,
    player: Player,
    started: bool,
    position_offset: Duration,
}

impl Audio {
    pub fn open(volume: f32) -> Result<Self> {
        let mut device =
            DeviceSinkBuilder::open_default_sink().context(AppError::AudioUnavailable)?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        player.set_volume(volume);
        Ok(Self {
            device,
            player,
            started: false,
            position_offset: Duration::ZERO,
        })
    }

    pub fn prepare(path: &Path) -> Result<PreparedAudio> {
        Decoder::try_from(
            File::open(path).with_context(|| format!("Cannot open {}", path.display()))?,
        )
        .context(AppError::InvalidAudio)
    }

    pub fn waveform(path: &Path, cancel: &AtomicBool) -> Result<Vec<f32>> {
        let source = Self::prepare(path)?;
        // Keep an energy envelope of the whole file in bounded memory. When the
        // buffer fills, merge neighboring windows instead of retaining samples.
        let mut windows: Vec<(f64, u64)> = Vec::with_capacity(8192);
        let mut window_size = 512u64;
        let (mut energy, mut count) = (0.0f64, 0u64);
        for sample in source {
            if count.is_multiple_of(4096) && cancel.load(Ordering::Relaxed) {
                return Ok(Vec::new());
            }
            let sample = f64::from(sample);
            if sample.is_finite() {
                energy += sample * sample;
            }
            count += 1;
            if count == window_size {
                windows.push((energy, count));
                (energy, count) = (0.0, 0);
                if windows.len() == 8192 {
                    for index in 0..4096 {
                        let (a, b) = (windows[index * 2], windows[index * 2 + 1]);
                        windows[index] = (a.0 + b.0, a.1 + b.1);
                    }
                    windows.truncate(4096);
                    window_size = window_size.saturating_mul(2);
                }
            }
        }
        if count > 0 {
            windows.push((energy, count));
        }
        Ok(windows
            .into_iter()
            .map(|(energy, count)| (energy / count as f64).sqrt() as f32)
            .collect())
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.play_prepared(Self::prepare(path)?, false);
        Ok(())
    }

    pub fn play_prepared(&mut self, source: PreparedAudio, paused: bool) {
        self.play_prepared_at(source, paused, Duration::ZERO);
    }

    pub fn prepare_at(path: &Path, position: Duration) -> Result<PreparedAudio> {
        let mut source = Self::prepare(path)?;
        source
            .try_seek(position)
            .context(AppError::SeekingUnavailable)?;
        Ok(source)
    }

    pub fn play_prepared_at(&mut self, source: PreparedAudio, paused: bool, position: Duration) {
        let volume = self.player.volume();
        self.player.stop();
        self.player = Player::connect_new(self.device.mixer());
        self.player.set_volume(volume);
        self.set_paused(paused);
        self.player.append(source);
        self.started = true;
        self.position_offset = position;
    }

    pub fn set_paused(&self, paused: bool) {
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
    }

    pub fn toggle(&self) {
        if self.player.is_paused() {
            self.player.play();
        } else {
            self.player.pause();
        }
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.started = false;
        self.position_offset = Duration::ZERO;
    }
    pub fn paused(&self) -> bool {
        self.player.is_paused()
    }
    pub fn has_track(&self) -> bool {
        self.started && !self.player.empty()
    }
    pub fn finished(&mut self) -> bool {
        if self.started && self.player.empty() {
            self.started = false;
            true
        } else {
            false
        }
    }
    pub fn position(&self) -> Duration {
        self.position_offset.saturating_add(self.player.get_pos())
    }
    pub fn seek(&mut self, position: Duration) -> Result<()> {
        self.player
            .try_seek(position)
            .context(AppError::SeekingUnavailable)?;
        self.position_offset = Duration::ZERO;
        Ok(())
    }
    pub fn volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }
}
