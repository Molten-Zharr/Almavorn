use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::{fs::File, path::Path, time::Duration};

pub struct Audio {
    device: MixerDeviceSink,
    player: Player,
    started: bool,
}

impl Audio {
    pub fn open(volume: f32) -> Result<Self> {
        let mut device =
            DeviceSinkBuilder::open_default_sink().context("Audio output unavailable")?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        player.set_volume(volume);
        Ok(Self {
            device,
            player,
            started: false,
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        let source = Decoder::try_from(
            File::open(path).with_context(|| format!("Cannot open {}", path.display()))?,
        )
        .context("Unsupported or damaged audio file")?;
        let volume = self.player.volume();
        self.player.stop();
        self.player = Player::connect_new(self.device.mixer());
        self.player.set_volume(volume);
        self.player.append(source);
        self.started = true;
        Ok(())
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
    }
    pub fn paused(&self) -> bool {
        self.player.is_paused()
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
        self.player.get_pos()
    }
    pub fn seek(&self, position: Duration) -> Result<()> {
        self.player
            .try_seek(position)
            .context("Seeking is not available for this file")
    }
    pub fn volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }
}
