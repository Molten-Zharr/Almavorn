//! Ten octave bands and preamp, applied before the player's volume control.
use crate::model::Language;
use rodio::{ChannelCount, Sample, SampleRate, Source, source::SeekError};
use serde::{Deserialize, Serialize};
use std::{
    f64::consts::TAU,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

pub const FREQUENCIES: [f64; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
pub const LABELS: [&str; 10] = [
    "32", "64", "125", "250", "500", "1K", "2K", "4K", "8K", "16K",
];
pub const MIN_GAIN: i8 = -12;
pub const MAX_GAIN: i8 = 12;
pub const SLIDERS: usize = 11;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqualizerSettings {
    pub enabled: bool,
    pub preamp: i8,
    pub bands: [i8; 10],
}

impl EqualizerSettings {
    pub fn normalize(&mut self) {
        self.preamp = self.preamp.clamp(MIN_GAIN, MAX_GAIN);
        for gain in &mut self.bands {
            *gain = (*gain).clamp(MIN_GAIN, MAX_GAIN);
        }
    }

    pub fn gain(&self, index: usize) -> Option<i8> {
        if index == 0 {
            Some(self.preamp)
        } else {
            self.bands.get(index - 1).copied()
        }
    }

    pub fn set_gain(&mut self, index: usize, gain: i8) {
        let gain = gain.clamp(MIN_GAIN, MAX_GAIN);
        if index == 0 {
            self.preamp = gain;
        } else if let Some(value) = self.bands.get_mut(index - 1) {
            *value = gain;
        }
    }

    pub fn preset(&self) -> Option<usize> {
        PRESETS
            .iter()
            .position(|preset| preset.preamp == self.preamp && preset.bands == self.bands)
    }
}

pub struct Preset {
    english: &'static str,
    russian: &'static str,
    pub preamp: i8,
    pub bands: [i8; 10],
}

impl Preset {
    pub fn name(&self, language: Language) -> &'static str {
        language.text(self.english, self.russian)
    }
}

pub const PRESETS: [Preset; 8] = [
    Preset {
        english: "Flat",
        russian: "Ровный",
        preamp: 0,
        bands: [0; 10],
    },
    Preset {
        english: "Dance",
        russian: "Танцевальный",
        preamp: -5,
        bands: [4, 5, 3, 0, 1, 2, 3, 2, 3, 4],
    },
    Preset {
        english: "Rock",
        russian: "Рок",
        preamp: -5,
        bands: [5, 4, 2, -1, -2, 0, 2, 4, 5, 4],
    },
    Preset {
        english: "Pop",
        russian: "Поп",
        preamp: -4,
        bands: [-1, 1, 3, 4, 3, 1, -1, -1, 1, 2],
    },
    Preset {
        english: "Classical",
        russian: "Классика",
        preamp: -3,
        bands: [3, 2, 1, 0, 0, 0, 0, 1, 2, 3],
    },
    Preset {
        english: "Bass boost",
        russian: "Усиление баса",
        preamp: -6,
        bands: [6, 5, 3, 1, 0, 0, 0, 0, 0, 0],
    },
    Preset {
        english: "Treble boost",
        russian: "Высокие частоты",
        preamp: -5,
        bands: [0, 0, 0, 0, 0, 0, 1, 3, 4, 5],
    },
    Preset {
        english: "Vocal",
        russian: "Вокал",
        preamp: -4,
        bands: [-3, -2, -1, 0, 2, 4, 4, 2, 0, -1],
    },
];

/// The callback never waits for the UI: it polls a version every 64 frames
/// and uses try_lock only when the settings actually change.
pub(crate) struct EqualizerControl {
    settings: Mutex<EqualizerSettings>,
    revision: AtomicU64,
}

impl EqualizerControl {
    pub(crate) fn new(settings: EqualizerSettings) -> Arc<Self> {
        Arc::new(Self {
            settings: Mutex::new(settings),
            revision: AtomicU64::new(0),
        })
    }

    pub(crate) fn update(&self, settings: &EqualizerSettings) {
        let mut normalized = settings.clone();
        normalized.normalize();
        let mut stored = self
            .settings
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *stored != normalized {
            *stored = normalized;
            self.revision.fetch_add(1, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Coefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coefficients {
    pub(crate) const IDENTITY: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    fn peak(frequency: f64, rate: f64, gain: i8) -> Self {
        if gain == 0 || frequency >= rate / 2.0 {
            return Self::IDENTITY;
        }
        // RBJ peaking EQ, Q = sqrt(2), roughly one octave per band.
        // https://www.w3.org/TR/audio-eq-cookbook/#peaking-eq
        let a = 10.0_f64.powf(f64::from(gain) / 40.0);
        let omega = TAU * frequency / rate;
        let alpha = omega.sin() / (2.0 * 2.0_f64.sqrt());
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * omega.cos() / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * omega.cos() / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    pub(crate) fn low_shelf(frequency: f64, rate: f64, gain: f64) -> Self {
        if gain == 0.0 || frequency >= rate / 2.0 {
            return Self::IDENTITY;
        }
        // RBJ low shelf, slope S = 1. Bass is independent of the EQ on/off state.
        // https://www.w3.org/TR/audio-eq-cookbook/
        let a = 10.0_f64.powf(gain / 40.0);
        let omega = TAU * frequency / rate;
        let c = omega.cos();
        let beta = omega.sin() * (2.0 * a).sqrt();
        let a0 = (a + 1.0) + (a - 1.0) * c + beta;
        Self {
            b0: a * ((a + 1.0) - (a - 1.0) * c + beta) / a0,
            b1: 2.0 * a * ((a - 1.0) - (a + 1.0) * c) / a0,
            b2: a * ((a + 1.0) - (a - 1.0) * c - beta) / a0,
            a1: -2.0 * ((a - 1.0) + (a + 1.0) * c) / a0,
            a2: ((a + 1.0) + (a - 1.0) * c - beta) / a0,
        }
    }

    pub(crate) fn approach(&mut self, target: Self, fraction: f64) {
        self.b0 += (target.b0 - self.b0) * fraction;
        self.b1 += (target.b1 - self.b1) * fraction;
        self.b2 += (target.b2 - self.b2) * fraction;
        self.a1 += (target.a1 - self.a1) * fraction;
        self.a2 += (target.a2 - self.a2) * fraction;
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct FilterState {
    z1: f64,
    z2: f64,
}

impl FilterState {
    pub(crate) fn process(&mut self, input: f64, c: Coefficients) -> f64 {
        let output = c.b0 * input + self.z1;
        self.z1 = c.b1 * input - c.a1 * output + self.z2;
        self.z2 = c.b2 * input - c.a2 * output;
        if self.z1.abs() < 1e-24 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-24 {
            self.z2 = 0.0;
        }
        output
    }
}

pub(crate) struct EqualizerSource<S> {
    inner: S,
    control: Arc<EqualizerControl>,
    settings: EqualizerSettings,
    revision: u64,
    coefficients: [Coefficients; 10],
    target: [Coefficients; 10],
    states: Vec<[FilterState; 10]>,
    channel: usize,
    rate: u32,
    frames: u64,
    ramp: u32,
    gain: f64,
    target_gain: f64,
}

impl<S: Source> EqualizerSource<S> {
    pub(crate) fn new(inner: S, control: Arc<EqualizerControl>) -> Self {
        let (settings, revision) = {
            let settings = control
                .settings
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            (settings.clone(), control.revision.load(Ordering::Acquire))
        };
        let rate = inner.sample_rate().get();
        let states = vec![[FilterState::default(); 10]; usize::from(inner.channels().get())];
        let mut source = Self {
            inner,
            control,
            settings,
            revision,
            coefficients: [Coefficients::IDENTITY; 10],
            target: [Coefficients::IDENTITY; 10],
            states,
            channel: 0,
            rate,
            frames: 0,
            ramp: 0,
            gain: 1.0,
            target_gain: 1.0,
        };
        source.targets();
        source.coefficients = source.target;
        source.gain = source.target_gain;
        source
    }

    fn targets(&mut self) {
        self.target = std::array::from_fn(|index| {
            Coefficients::peak(
                FREQUENCIES[index],
                f64::from(self.rate),
                if self.settings.enabled {
                    self.settings.bands[index]
                } else {
                    0
                },
            )
        });
        self.target_gain = if self.settings.enabled {
            10.0_f64.powf(f64::from(self.settings.preamp) / 20.0)
        } else {
            1.0
        };
    }

    fn frame(&mut self) {
        if self.frames.is_multiple_of(64) {
            let revision = self.control.revision.load(Ordering::Acquire);
            if revision != self.revision {
                let snapshot = self
                    .control
                    .settings
                    .try_lock()
                    .ok()
                    .map(|settings| settings.clone());
                if let Some(settings) = snapshot {
                    self.settings = settings;
                    self.revision = revision;
                    self.targets();
                    self.ramp = (self.rate / 50).max(1);
                }
            }
        }
        let channels = usize::from(self.inner.channels().get());
        let rate = self.inner.sample_rate().get();
        if rate != self.rate || channels != self.states.len() {
            self.rate = rate;
            self.states = vec![[FilterState::default(); 10]; channels];
            self.targets();
            self.coefficients = self.target;
            self.gain = self.target_gain;
            self.ramp = 0;
        }
        if self.ramp > 0 {
            let fraction = 1.0 / f64::from(self.ramp);
            for (current, target) in self.coefficients.iter_mut().zip(self.target) {
                current.approach(target, fraction);
            }
            self.gain += (self.target_gain - self.gain) * fraction;
            self.ramp -= 1;
            if self.ramp == 0 && self.bypass() {
                self.states.fill([FilterState::default(); 10]);
            }
        }
        self.frames = self.frames.wrapping_add(1);
    }

    fn bypass(&self) -> bool {
        self.ramp == 0
            && (!self.settings.enabled
                || (self.settings.preamp == 0 && self.settings.bands == [0; 10]))
    }
}

impl<S: Source> Iterator for EqualizerSource<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.channel == 0 {
            self.frame();
        }
        let input = self.inner.next()?;
        let mut output = f64::from(input);
        if !self.bypass() {
            output *= self.gain;
            for (state, coefficients) in self.states[self.channel].iter_mut().zip(self.coefficients)
            {
                output = state.process(output, coefficients);
            }
        }
        self.channel = (self.channel + 1) % self.states.len();
        Some(output as Sample)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<S: Source> Source for EqualizerSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }
    fn channels(&self) -> ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.inner.try_seek(position)?;
        self.states.fill([FilterState::default(); 10]);
        self.channel = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;

    fn buffer(channels: u16, rate: u32, data: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(
            NonZero::new(channels).unwrap(),
            NonZero::new(rate).unwrap(),
            data,
        )
    }

    fn tone(frequency: f64, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| (0.1 * (TAU * frequency * frame as f64 / f64::from(rate)).sin()) as f32)
            .collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>()
            / samples.len() as f64)
            .sqrt()
    }

    #[test]
    fn disabled_and_flat_equalizers_are_exact_bypasses_and_keep_source_metadata() {
        let data = tone(1000.0, 44100, 44100);
        for settings in [
            EqualizerSettings {
                enabled: false,
                preamp: 12,
                bands: [12; 10],
            },
            EqualizerSettings {
                enabled: true,
                ..Default::default()
            },
        ] {
            let source = buffer(1, 44100, data.clone());
            let duration = source.total_duration();
            let equalized = EqualizerSource::new(source, EqualizerControl::new(settings));
            assert_eq!(equalized.channels().get(), 1);
            assert_eq!(equalized.sample_rate().get(), 44100);
            assert_eq!(equalized.total_duration(), duration);
            assert_eq!(equalized.collect::<Vec<_>>(), data);
        }
    }

    #[test]
    fn preamp_changes_amplitude_by_the_requested_decibels() {
        let data = tone(440.0, 48000, 48000);
        for preamp in [-12, -6, 6, 12] {
            let settings = EqualizerSettings {
                enabled: true,
                preamp,
                ..Default::default()
            };
            let output: Vec<_> = EqualizerSource::new(
                buffer(1, 48000, data.clone()),
                EqualizerControl::new(settings),
            )
            .collect();
            let gain = 20.0 * (rms(&output) / rms(&data)).log10();
            assert!((gain - f64::from(preamp)).abs() < 0.001, "{preamp}: {gain}");
        }
    }

    #[test]
    fn a_band_boosts_its_center_frequency_without_mixing_stereo_channels() {
        let mut settings = EqualizerSettings {
            enabled: true,
            ..Default::default()
        };
        settings.bands[5] = 6;
        for (frequency, expected) in [(1000.0, 6.0), (100.0, 0.0)] {
            let data = tone(frequency, 48000, 48000);
            let stereo: Vec<_> = data.iter().flat_map(|sample| [*sample, 0.0]).collect();
            let output: Vec<_> = EqualizerSource::new(
                buffer(2, 48000, stereo),
                EqualizerControl::new(settings.clone()),
            )
            .collect();
            assert_eq!(output.len(), data.len() * 2);
            assert!(
                output
                    .iter()
                    .skip(1)
                    .step_by(2)
                    .all(|sample| *sample == 0.0)
            );
            let left: Vec<_> = output.iter().step_by(2).copied().collect();
            let gain = 20.0 * (rms(&left[12000..]) / rms(&data[12000..])).log10();
            assert!((gain - expected).abs() < 0.1, "{frequency}: {gain}");
        }
    }

    #[test]
    fn changes_affect_the_running_source_and_bypass_returns_to_exact_dry_audio() {
        let data = tone(1000.0, 48000, 48000);
        let control = EqualizerControl::new(EqualizerSettings::default());
        let mut source = EqualizerSource::new(buffer(1, 48000, data.clone()), control.clone());
        assert_eq!(
            source.by_ref().take(12000).collect::<Vec<_>>(),
            data[..12000]
        );
        let mut settings = EqualizerSettings {
            enabled: true,
            preamp: -6,
            ..Default::default()
        };
        control.update(&settings);
        let output: Vec<_> = source.by_ref().take(12000).collect();
        let gain = 20.0 * (rms(&output[6000..]) / rms(&data[18000..24000])).log10();
        assert!((gain + 6.0).abs() < 0.001);
        settings.bands[5] = 6;
        control.update(&settings);
        let output: Vec<_> = source.by_ref().take(12000).collect();
        let gain = 20.0 * (rms(&output[6000..]) / rms(&data[30000..36000])).log10();
        assert!(gain.abs() < 0.01);
        settings.enabled = false;
        control.update(&settings);
        let output: Vec<_> = source.by_ref().take(12000).collect();
        assert_eq!(&output[6000..], &data[42000..48000]);
        assert_eq!(source.count(), 0);
    }

    #[test]
    fn bands_above_nyquist_stay_finite_and_seeking_clears_filter_history() {
        for rate in [8000, 22050, 44100, 192000] {
            for gain in [MIN_GAIN, MAX_GAIN] {
                let data = tone(500.0, rate, rate as usize);
                let settings = EqualizerSettings {
                    enabled: true,
                    preamp: -12,
                    bands: [gain; 10],
                };
                let mut source = EqualizerSource::new(
                    buffer(1, rate, data.clone()),
                    EqualizerControl::new(settings.clone()),
                );
                let first: Vec<_> = source.by_ref().take(1000).collect();
                assert!(first.iter().all(|sample| sample.is_finite()));
                source.try_seek(Duration::ZERO).unwrap();
                assert_eq!(source.by_ref().take(1000).collect::<Vec<_>>(), first);
                assert!(source.all(|sample| sample.is_finite()));
            }
        }
    }
}
