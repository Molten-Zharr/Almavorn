//! Independent rate, tempo and pitch controls plus a bass shelf.
use crate::{
    equalizer::{Coefficients, FilterState},
    model::Language,
};
use rodio::{ChannelCount, SampleRate, Source, source::SeekError};
use serde::{Deserialize, Serialize};
use soundtouch::SoundTouch;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const SLIDERS: usize = 4;
pub const MIN_PERCENT: u16 = 50;
pub const MAX_PERCENT: u16 = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TunerSettings {
    pub speed: u16,
    pub tempo: u16,
    pub pitch: u16,
    pub reverse: bool,
    pub bass: u16,
}

impl Default for TunerSettings {
    fn default() -> Self {
        Self {
            speed: 100,
            tempo: 100,
            pitch: 100,
            reverse: false,
            bass: 0,
        }
    }
}

impl TunerSettings {
    pub fn normalize(&mut self) {
        self.speed = self.speed.clamp(MIN_PERCENT, MAX_PERCENT);
        self.tempo = self.tempo.clamp(MIN_PERCENT, MAX_PERCENT);
        self.pitch = self.pitch.clamp(MIN_PERCENT, MAX_PERCENT);
        self.bass = self.bass.min(100);
    }

    pub fn value(&self, index: usize) -> Option<u16> {
        [self.speed, self.tempo, self.pitch, self.bass]
            .get(index)
            .copied()
    }

    pub fn set_value(&mut self, index: usize, value: u16) {
        let value = value.clamp(Self::minimum(index), Self::maximum(index));
        match index {
            0 => self.speed = value,
            1 => self.tempo = value,
            2 => self.pitch = value,
            3 => self.bass = value,
            _ => {}
        }
    }

    pub const fn minimum(index: usize) -> u16 {
        if index == 3 { 0 } else { MIN_PERCENT }
    }
    pub const fn maximum(index: usize) -> u16 {
        if index == 3 { 100 } else { MAX_PERCENT }
    }
    pub const fn normal(index: usize) -> u16 {
        if index == 3 { 0 } else { 100 }
    }

    pub fn label(index: usize, language: Language) -> &'static str {
        match index {
            0 => language.text("Speed", "Скорость"),
            1 => language.text("Tempo", "Темп"),
            2 => language.text("Pitch", "Тональность"),
            _ => language.text("Bass", "Бас"),
        }
    }

    pub fn description(index: usize, language: Language) -> &'static str {
        match index {
            0 => language.text(
                "Changes duration and pitch together",
                "Меняет длительность и высоту звука вместе",
            ),
            1 => language.text(
                "Changes duration while preserving pitch",
                "Меняет длительность с сохранением тона",
            ),
            2 => language.text(
                "Changes pitch while preserving duration",
                "Меняет высоту звука с сохранением длительности",
            ),
            _ => language.text(
                "Low frequencies: 0–100% = 0–12 dB",
                "Низкие частоты: 0–100% = 0–12 дБ",
            ),
        }
    }

    fn time_factor(&self) -> f64 {
        f64::from(self.speed) * f64::from(self.tempo) / 10000.0
    }
    fn stretch(&self) -> bool {
        self.speed != 100 || self.tempo != 100 || self.pitch != 100
    }
}

pub(crate) struct TunerControl {
    settings: Mutex<TunerSettings>,
    revision: AtomicU64,
}

impl TunerControl {
    pub(crate) fn new(settings: TunerSettings) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(settings),
            revision: AtomicU64::new(0),
        })
    }

    pub(crate) fn update(&self, settings: &TunerSettings) {
        let mut normalized = settings.clone();
        normalized.normalize();
        let mut stored = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if *stored != normalized {
            *stored = normalized;
            self.revision.fetch_add(1, Ordering::Release);
        }
    }
}

/// Owns one SoundTouch processor. It is only accessed by the source iterator.
/// Output and input buffers stay bounded regardless of track duration.
pub(crate) struct TunerSource<S> {
    inner: S,
    control: Arc<TunerControl>,
    settings: TunerSettings,
    revision: u64,
    processor: SoundTouch,
    processing: bool,
    ended: bool,
    input: Vec<f32>,
    output: Vec<f32>,
    output_len: usize,
    output_index: usize,
    output_factor: f64,
    channels: ChannelCount,
    rate: SampleRate,
    channel: usize,
    frames: u64,
    position_frames: f64,
    position: Arc<AtomicU64>,
    reverse: bool,
    shelf: Coefficients,
    target_shelf: Coefficients,
    shelf_ramp: u32,
    states: Vec<FilterState>,
}

impl<S: Source> TunerSource<S> {
    pub(crate) fn new(
        inner: S,
        control: Arc<TunerControl>,
        position: Arc<AtomicU64>,
        start: Duration,
        reverse: bool,
    ) -> Self {
        let (settings, revision) = {
            let settings = control.settings.lock().unwrap_or_else(|e| e.into_inner());
            (settings.clone(), control.revision.load(Ordering::Acquire))
        };
        let channels = inner.channels();
        let rate = inner.sample_rate();
        let mut processor = SoundTouch::new();
        processor
            .set_channels(u32::from(channels.get()))
            .set_sample_rate(rate.get());
        Self::configure(&mut processor, &settings);
        let shelf = Coefficients::low_shelf(
            120.0,
            f64::from(rate.get()),
            f64::from(settings.bass) * 0.12,
        );
        position.store(
            start.as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        Self {
            processing: settings.stretch(),
            inner,
            control,
            settings,
            revision,
            processor,
            ended: false,
            input: Vec::with_capacity(512 * usize::from(channels.get())),
            output: vec![0.0; 256 * usize::from(channels.get())],
            output_len: 0,
            output_index: 0,
            output_factor: 1.0,
            channels,
            rate,
            channel: 0,
            frames: 0,
            position_frames: start.as_secs_f64() * f64::from(rate.get()),
            position,
            reverse,
            shelf,
            target_shelf: shelf,
            shelf_ramp: 0,
            states: vec![FilterState::default(); usize::from(channels.get())],
        }
    }

    fn configure(processor: &mut SoundTouch, settings: &TunerSettings) {
        processor
            .set_rate(f64::from(settings.speed) / 100.0)
            .set_tempo(f64::from(settings.tempo) / 100.0)
            .set_pitch(f64::from(settings.pitch) / 100.0);
    }

    fn refresh(&mut self) {
        let revision = self.control.revision.load(Ordering::Acquire);
        if revision == self.revision {
            return;
        }
        let Ok(settings) = self.control.settings.try_lock() else {
            return;
        };
        self.settings = settings.clone();
        self.revision = self.control.revision.load(Ordering::Acquire);
        drop(settings);
        Self::configure(&mut self.processor, &self.settings);
        self.processing |= self.settings.stretch();
        self.target_shelf = Coefficients::low_shelf(
            120.0,
            f64::from(self.rate.get()),
            f64::from(self.settings.bass) * 0.12,
        );
        self.shelf_ramp = (self.rate.get() / 50).max(1);
    }

    fn refill(&mut self) -> bool {
        let channels = usize::from(self.channels.get());
        loop {
            let frames = self.processor.receive_samples(&mut self.output, 256);
            if frames > 0 {
                self.output_len = frames * channels;
                self.output_index = 0;
                self.output_factor = self.settings.time_factor();
                return true;
            }
            if self.ended {
                return false;
            }
            self.input.clear();
            for sample in self.inner.by_ref().take(512 * channels) {
                self.input.push(sample);
            }
            let frames = self.input.len() / channels;
            if frames > 0 {
                self.processor.put_samples(&self.input, frames);
            }
            if self.input.len() < 512 * channels {
                self.ended = true;
                self.processor.flush();
            }
        }
    }
}

impl<S: Source> Iterator for TunerSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.channel == 0 && self.frames.is_multiple_of(64) {
            self.refresh();
        }
        let (sample, factor) = if self.processing {
            if self.output_index == self.output_len && !self.refill() {
                return None;
            }
            let sample = self.output[self.output_index];
            self.output_index += 1;
            (sample, self.output_factor)
        } else {
            (self.inner.next()?, 1.0)
        };
        if self.channel == 0 && self.shelf_ramp > 0 {
            self.shelf
                .approach(self.target_shelf, 1.0 / f64::from(self.shelf_ramp));
            self.shelf_ramp -= 1;
        }
        let sample = self.states[self.channel].process(f64::from(sample), self.shelf) as f32;
        self.channel += 1;
        if self.channel == usize::from(self.channels.get()) {
            self.channel = 0;
            self.frames += 1;
            self.position_frames =
                (self.position_frames + if self.reverse { -factor } else { factor }).max(0.0);
            if let Some(duration) = self.inner.total_duration() {
                self.position_frames = self
                    .position_frames
                    .min(duration.as_secs_f64() * f64::from(self.rate.get()));
            }
            if self.frames.is_multiple_of(64) {
                self.position.store(
                    (self.position_frames * 1000.0 / f64::from(self.rate.get())) as u64,
                    Ordering::Relaxed,
                );
            }
        }
        Some(sample)
    }
}

impl<S: Source> Source for TunerSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner
            .total_duration()
            .map(|duration| duration.div_f64(self.settings.time_factor()))
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(position)?;
        self.processor.clear();
        self.output_index = 0;
        self.output_len = 0;
        self.ended = false;
        self.channel = 0;
        self.position_frames = position.as_secs_f64() * f64::from(self.rate.get());
        self.position.store(
            position.as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        self.states.fill(FilterState::default());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::f64::consts::TAU;

    const RATE: u32 = 44100;

    fn signal(frequency: f64, channels: u16) -> SamplesBuffer {
        let samples: Vec<f32> = (0..RATE * 3)
            .flat_map(|frame| {
                let sample =
                    (TAU * frequency * f64::from(frame) / f64::from(RATE)).sin() as f32 * 0.05;
                (0..channels).map(move |channel| if channel == 0 { sample } else { 0.0 })
            })
            .collect();
        SamplesBuffer::new(
            ChannelCount::new(channels).unwrap(),
            SampleRate::new(RATE).unwrap(),
            samples,
        )
    }

    fn source(
        settings: TunerSettings,
        frequency: f64,
        channels: u16,
    ) -> TunerSource<SamplesBuffer> {
        TunerSource::new(
            signal(frequency, channels),
            TunerControl::new(settings),
            Arc::new(AtomicU64::new(0)),
            Duration::ZERO,
            false,
        )
    }

    fn frequency(samples: &[f32]) -> f64 {
        let samples = &samples[RATE as usize / 4..samples.len() - RATE as usize / 4];
        let crossings = samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        crossings as f64 * f64::from(RATE) / samples.len() as f64
    }

    fn rms(samples: &[f32]) -> f64 {
        let samples = &samples[RATE as usize / 4..samples.len() - RATE as usize / 4];
        (samples
            .iter()
            .map(|&sample| f64::from(sample).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt()
    }

    #[test]
    fn defaults_bypass_exactly_and_speed_tempo_and_pitch_have_independent_effects() {
        let plain = signal(440.0, 1).collect::<Vec<_>>();
        assert_eq!(
            source(TunerSettings::default(), 440.0, 1).collect::<Vec<_>>(),
            plain
        );
        for (settings, duration, pitch) in [
            (
                TunerSettings {
                    speed: 150,
                    ..Default::default()
                },
                2.0,
                660.0,
            ),
            (
                TunerSettings {
                    tempo: 150,
                    ..Default::default()
                },
                2.0,
                440.0,
            ),
            (
                TunerSettings {
                    pitch: 150,
                    ..Default::default()
                },
                3.0,
                660.0,
            ),
            (
                TunerSettings {
                    speed: 125,
                    tempo: 80,
                    pitch: 150,
                    ..Default::default()
                },
                3.0,
                825.0,
            ),
        ] {
            let output = source(settings.clone(), 440.0, 1).collect::<Vec<_>>();
            assert!(output.iter().all(|sample| sample.is_finite()));
            let actual_duration = output.len() as f64 / f64::from(RATE);
            assert!(
                (actual_duration - duration).abs() < 0.035,
                "{settings:?}: duration {actual_duration}"
            );
            let actual_pitch = frequency(&output);
            assert!(
                (actual_pitch - pitch).abs() / pitch < 0.015,
                "{settings:?}: pitch {actual_pitch}"
            );
        }
    }

    #[test]
    fn bass_boosts_low_frequencies_without_boosting_treble_or_crossing_channels() {
        let mut bass = source(
            TunerSettings {
                bass: 100,
                ..Default::default()
            },
            32.0,
            2,
        );
        assert_eq!(bass.channels().get(), 2);
        assert_eq!(bass.sample_rate().get(), RATE);
        let output = bass.by_ref().collect::<Vec<_>>();
        assert!(
            output
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[1] == 0.0)
        );
        let left: Vec<_> = output
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect();
        let dry = signal(32.0, 1).collect::<Vec<_>>();
        assert!((rms(&left) / rms(&dry) - 3.98).abs() < 0.1);
        let high = source(
            TunerSettings {
                bass: 100,
                ..Default::default()
            },
            4000.0,
            1,
        )
        .collect::<Vec<_>>();
        let high_dry = signal(4000.0, 1).collect::<Vec<_>>();
        assert!((rms(&high) / rms(&high_dry) - 1.0).abs() < 0.01);
    }

    #[test]
    fn live_changes_and_seeks_update_original_media_time_without_losing_stereo() {
        let mut source = source(TunerSettings::default(), 440.0, 2);
        source.by_ref().take((RATE / 2) as usize * 2).for_each(drop);
        assert!((source.position.load(Ordering::Relaxed) as i64 - 500).abs() < 3);
        source.control.update(&TunerSettings {
            speed: 150,
            tempo: 125,
            pitch: 80,
            ..Default::default()
        });
        let output: Vec<_> = source.by_ref().take(RATE as usize * 2).collect();
        assert!(
            output
                .as_chunks::<2>()
                .0
                .iter()
                .all(|frame| frame[1] == 0.0)
        );
        let left: Vec<_> = output
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect();
        assert!((frequency(&left) - 528.0).abs() < 8.0);
        assert!((source.position.load(Ordering::Relaxed) as i64 - 2375).abs() < 10);
        source.try_seek(Duration::from_millis(200)).unwrap();
        assert_eq!(source.position.load(Ordering::Relaxed), 200);
        assert_eq!(source.output_len, 0);
        source.control.update(&TunerSettings::default());
        let output: Vec<_> = source.by_ref().take(RATE as usize * 2).collect();
        assert_eq!(output.len(), RATE as usize * 2);
        assert!((source.position.load(Ordering::Relaxed) as i64 - 1200).abs() < 10);
    }

    #[test]
    fn extreme_settings_flush_to_completion_with_bounded_buffers_and_support_multichannel() {
        for percent in [50, 200] {
            let mut source = source(
                TunerSettings {
                    speed: percent,
                    tempo: percent,
                    pitch: percent,
                    bass: 100,
                    ..Default::default()
                },
                440.0,
                4,
            );
            let mut length = 0;
            for sample in source.by_ref() {
                assert!(sample.is_finite());
                length += 1;
                assert!(length <= RATE as usize * 4 * 13);
            }
            let expected = 3.0 / (f64::from(percent) / 100.0).powi(2);
            assert!((length as f64 / (4.0 * f64::from(RATE)) - expected).abs() < 0.04);
            assert!(source.input.len() <= 512 * 4);
            assert_eq!(source.output.len(), 256 * 4);
            assert!(source.next().is_none());
        }
    }

    #[test]
    fn reverse_timeline_decreases_even_with_independent_tempo() {
        let cache = crate::audio::PcmCache::decode(
            signal(440.0, 1),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        let reverse = cache.source(None);
        let position = Arc::new(AtomicU64::new(3000));
        let mut source = TunerSource::new(
            reverse,
            TunerControl::new(TunerSettings {
                tempo: 150,
                reverse: true,
                ..Default::default()
            }),
            position.clone(),
            Duration::from_secs(3),
            true,
        );
        let output: Vec<_> = source.by_ref().take(RATE as usize).collect();
        assert!((frequency(&output) - 440.0).abs() < 7.0);
        assert!((position.load(Ordering::Relaxed) as i64 - 1500).abs() < 3);
        let rest: Vec<_> = source.collect();
        assert!(((output.len() + rest.len()) as f64 / f64::from(RATE) - 2.0).abs() < 0.035);
    }
}
