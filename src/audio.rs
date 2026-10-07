mod reverse;
use crate::equalizer::{EqualizerControl, EqualizerSettings, EqualizerSource};
use crate::errors::AppError;
use crate::tuner::{TunerControl, TunerSettings, TunerSource};
use anyhow::{Context, Result};
pub(crate) use reverse::PcmCache;
use rodio::{
    ChannelCount, Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source,
    source::{SeekError, UniformSourceIterator},
};
use std::{
    fs::File,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

pub struct PreparedAudio {
    source: Box<dyn Source + Send>,
    pub(crate) cache: Option<Arc<PcmCache>>,
    position: Duration,
    reverse: bool,
}

impl PreparedAudio {
    pub(crate) fn reversed(&self) -> bool {
        self.reverse
    }
}

impl Iterator for PreparedAudio {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        self.source.next()
    }
}

impl Source for PreparedAudio {
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn channels(&self) -> ChannelCount {
        self.source.channels()
    }
    fn sample_rate(&self) -> SampleRate {
        self.source.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> std::result::Result<(), SeekError> {
        self.source.try_seek(position)?;
        self.position = position;
        Ok(())
    }
}

pub struct Audio {
    device: MixerDeviceSink,
    player: Player,
    started: bool,
    position: Arc<AtomicU64>,
    reverse: bool,
    equalizer: Arc<EqualizerControl>,
    tuner: Arc<TunerControl>,
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
            position: Arc::new(AtomicU64::new(0)),
            reverse: false,
            equalizer: EqualizerControl::new(EqualizerSettings::default()),
            tuner: TunerControl::new(TunerSettings::default()),
        })
    }

    pub fn prepare(path: &Path) -> Result<PreparedAudio> {
        let source = Decoder::try_from(
            File::open(path).with_context(|| format!("Cannot open {}", path.display()))?,
        )
        .context(AppError::InvalidAudio)?;
        Ok(PreparedAudio {
            source: Box::new(source),
            cache: None,
            position: Duration::ZERO,
            reverse: false,
        })
    }

    pub(crate) fn prepare_tuned(
        path: &Path,
        position: Option<Duration>,
        reverse: bool,
        cache: Option<Arc<PcmCache>>,
        cancel: &AtomicBool,
    ) -> Result<PreparedAudio> {
        if reverse {
            let cache = match cache {
                Some(cache) => cache,
                None => PcmCache::decode(Self::prepare(path)?, cancel)?,
            };
            let source = cache.source(position);
            let position = source.position();
            Ok(PreparedAudio {
                source: Box::new(source),
                cache: Some(cache),
                position,
                reverse: true,
            })
        } else if let Some(position) = position {
            Self::prepare_at(path, position)
        } else {
            Self::prepare(path)
        }
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
        self.reverse = source.reverse;
        let start = if self.reverse {
            source.position
        } else {
            position
        };
        self.position = Arc::new(AtomicU64::new(
            start.as_millis().min(u128::from(u64::MAX)) as u64
        ));
        let channels = ChannelCount::new(source.channels().get().min(16)).unwrap();
        let rate = source.sample_rate();
        let source = UniformSourceIterator::new(source, channels, rate);
        let source = TunerSource::new(
            source,
            self.tuner.clone(),
            self.position.clone(),
            start,
            self.reverse,
        );
        self.player
            .append(EqualizerSource::new(source, self.equalizer.clone()));
        self.started = true;
    }

    pub fn set_paused(&self, paused: bool) {
        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }
    }

    pub fn equalizer(&self, settings: &EqualizerSettings) {
        self.equalizer.update(settings);
    }

    pub fn tuner(&self, settings: &TunerSettings) {
        self.tuner.update(settings);
    }
    pub(crate) fn reversed(&self) -> bool {
        self.reverse
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
        self.position = Arc::new(AtomicU64::new(0));
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
        Duration::from_millis(self.position.load(Ordering::Relaxed))
    }
    pub fn seek(&mut self, position: Duration) -> Result<()> {
        self.player
            .try_seek(position)
            .context(AppError::SeekingUnavailable)?;
        Ok(())
    }
    pub fn volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn decoder_prepares_both_directions_and_reuses_reverse_cache_for_original_time_seeks() {
        let mut file = tempfile::Builder::new().suffix(".wav").tempfile().unwrap();
        let frames = 2400u32;
        let length = frames * 4;
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&(36 + length).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&8000u32.to_le_bytes());
        bytes.extend_from_slice(&32000u32.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&length.to_le_bytes());
        for frame in 0..frames {
            bytes.extend_from_slice(&(frame as i16).to_le_bytes());
            bytes.extend_from_slice(&(-(frame as i16)).to_le_bytes());
        }
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        let cancel = AtomicBool::new(false);
        let forward = Audio::prepare_tuned(
            file.path(),
            Some(Duration::from_millis(100)),
            false,
            None,
            &cancel,
        )
        .unwrap();
        assert!(!forward.reversed());
        assert_eq!(forward.total_duration(), Some(Duration::from_millis(300)));
        assert_eq!(
            forward.take(2).collect::<Vec<_>>(),
            vec![800.0 / 32768.0, -800.0 / 32768.0]
        );
        let mut reverse = Audio::prepare_tuned(file.path(), None, true, None, &cancel).unwrap();
        assert!(reverse.reversed());
        assert_eq!(reverse.position, Duration::from_millis(300));
        assert_eq!(
            reverse.by_ref().take(2).collect::<Vec<_>>(),
            vec![2399.0 / 32768.0, -2399.0 / 32768.0]
        );
        let cache = reverse.cache.clone().unwrap();
        assert!(
            Audio::prepare_tuned(file.path(), None, true, None, &AtomicBool::new(true)).is_err()
        );
        let path = file.path().to_owned();
        file.close().unwrap();
        let cached = Audio::prepare_tuned(
            &path,
            Some(Duration::from_millis(200)),
            true,
            Some(cache.clone()),
            &cancel,
        )
        .unwrap();
        assert!(Arc::ptr_eq(cached.cache.as_ref().unwrap(), &cache));
        assert_eq!(
            cached.take(2).collect::<Vec<_>>(),
            vec![1599.0 / 32768.0, -1599.0 / 32768.0]
        );
    }
}
