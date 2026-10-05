use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::{fs::File, io::BufReader, path::Path, time::Duration};

pub type PreparedAudio = Decoder<BufReader<File>>;

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

    pub fn prepare(path: &Path) -> Result<PreparedAudio> {
        Decoder::try_from(
            File::open(path).with_context(|| format!("Cannot open {}", path.display()))?,
        )
        .context("Unsupported or damaged audio file")
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.play_prepared(Self::prepare(path)?, false);
        Ok(())
    }

    pub fn play_prepared(&mut self, source: PreparedAudio, paused: bool) {
        let volume = self.player.volume();
        self.player.stop();
        self.player = Player::connect_new(self.device.mixer());
        self.player.set_volume(volume);
        self.set_paused(paused);
        self.player.append(source);
        self.started = true;
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
