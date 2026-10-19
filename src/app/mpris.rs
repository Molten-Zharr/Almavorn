use super::App;
use crate::{
    input::Action,
    model::RepeatMode,
    mpris::{Bridge, Command, Snapshot, TrackInfo},
};
use anyhow::Result;

impl App {
    pub(super) fn start_mpris(&mut self) {
        self.mpris = Some(Bridge::start(self.runtime(), self.mpris_snapshot()));
    }

    pub(super) fn tick_mpris_commands(&mut self) {
        let Some(mut bridge) = self.mpris.take() else {
            return;
        };
        for _ in 0..16 {
            let Ok(incoming) = bridge.commands.try_recv() else {
                break;
            };
            // A caller that already timed out must not cause a late action.
            if incoming.reply.is_closed() {
                continue;
            }
            let result = self
                .apply_mpris(incoming.command)
                .map_err(|error| zbus::fdo::Error::Failed(error.to_string()));
            bridge.publish(self.mpris_snapshot());
            let _ = incoming.reply.send(result);
            if self.view.quit {
                break;
            }
        }
        self.mpris = Some(bridge);
    }

    pub(super) fn publish_mpris(&mut self) {
        if self.mpris.as_ref().is_some_and(Bridge::due) {
            let snapshot = self.mpris_snapshot();
            if let Some(bridge) = &mut self.mpris {
                bridge.publish(snapshot);
            }
        }
    }

    fn mpris_snapshot(&self) -> Snapshot {
        let tuner = self.tuner_settings();
        let active = self.playback_active() || self.preparing_playback();
        let loop_status = if self.marked_playback_count().is_some() {
            "Playlist"
        } else {
            match self.settings.preferences.repeat {
                RepeatMode::Off => "None",
                RepeatMode::Playlist => "Playlist",
                RepeatMode::Track => "Track",
            }
        };
        Snapshot {
            track: self.playback.current.as_ref().map(|track| TrackInfo {
                id: format!("/org/mpris/MediaPlayer2/track/t{}", track.id.unsigned_abs()),
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                url: url::Url::from_file_path(&track.path).ok().map(String::from),
                length: micros(track.duration_ms),
            }),
            status: if !active {
                "Stopped"
            } else if self.paused() {
                "Paused"
            } else {
                "Playing"
            },
            position: micros(
                self.position_ms().min(
                    self.playback
                        .current
                        .as_ref()
                        .map_or(0, |track| track.duration_ms),
                ),
            ),
            seek_serial: self.playback.seek_serial,
            rate: tuner.time_factor() * if tuner.reverse { -1.0 } else { 1.0 },
            volume: f64::from(self.settings.preferences.volume),
            loop_status,
            shuffle: self.settings.preferences.shuffle,
            can_play: self.allowed(Action::TogglePlay),
            can_pause: active,
            can_next: self.allowed(Action::Next),
            can_previous: self.allowed(Action::Previous),
            can_seek: self.playback.current.is_some() && active,
        }
    }

    fn apply_mpris(&mut self, command: Command) -> Result<()> {
        let active = self.playback_active() || self.preparing_playback();
        let action = match command {
            Command::Play if !active || self.paused() => Some(Action::TogglePlay),
            Command::Pause if active && !self.paused() => Some(Action::TogglePlay),
            Command::PlayPause => Some(Action::TogglePlay),
            Command::Stop => Some(Action::Stop),
            Command::Next => Some(Action::Next),
            Command::Previous => Some(Action::Previous),
            Command::Quit => Some(Action::Quit),
            Command::Volume(value) => {
                self.set_volume(value.clamp(0.0, 1.0) as f32)?;
                None
            }
            Command::Loop(value) => {
                self.set_repeat(match value.as_str() {
                    "Track" => RepeatMode::Track,
                    "Playlist" => RepeatMode::Playlist,
                    _ => RepeatMode::Off,
                })?;
                None
            }
            Command::Shuffle(value) if value != self.settings.preferences.shuffle => {
                Some(Action::ToggleShuffle)
            }
            Command::Rate(0.0) if active && !self.paused() => Some(Action::TogglePlay),
            Command::Rate(value) if value != 0.0 => {
                self.set_mpris_rate(value)?;
                None
            }
            Command::Seek(offset) if active => {
                if let Some(track) = &self.playback.current {
                    let length = micros(track.duration_ms);
                    let position = i128::from(micros(self.position_ms())) + i128::from(offset);
                    if position > i128::from(length) {
                        if self.allowed(Action::Next) {
                            self.action(Action::Next)?;
                        }
                    } else {
                        self.seek_to((position.max(0) / 1000) as u64);
                    }
                }
                None
            }
            Command::SetPosition(id, position) if active => {
                if let Some(track) = &self.playback.current
                    && id == format!("/org/mpris/MediaPlayer2/track/t{}", track.id.unsigned_abs())
                    && position >= 0
                    && position <= micros(track.duration_ms)
                {
                    self.seek_to((position / 1000) as u64);
                }
                None
            }
            _ => None,
        };
        if let Some(action) = action
            && self.allowed(action)
        {
            self.action(action)?;
        }
        Ok(())
    }
}

fn micros(milliseconds: u64) -> i64 {
    milliseconds.saturating_mul(1000).min(i64::MAX as u64) as i64
}
