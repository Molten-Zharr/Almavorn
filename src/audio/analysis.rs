//! A bounded, nonblocking tap of the samples that actually reach the player.
use rodio::{ChannelCount, SampleRate, Source, source::SeekError};
use std::{
    f32::consts::TAU,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const SPECTRUM_BANDS: usize = 24;
pub const WAVE_SAMPLES: usize = 64;
const WINDOW: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct AnalysisFrame {
    pub sequence: u64,
    pub beats: u64,
    pub rms: f32,
    pub bass: f32,
    pub treble: f32,
    pub bands: [f32; SPECTRUM_BANDS],
    pub wave: [f32; WAVE_SAMPLES],
}
impl Default for AnalysisFrame {
    fn default() -> Self {
        Self {
            sequence: 0,
            beats: 0,
            rms: 0.0,
            bass: 0.0,
            treble: 0.0,
            bands: [0.0; SPECTRUM_BANDS],
            wave: [0.0; WAVE_SAMPLES],
        }
    }
}

pub(super) struct AudioAnalysis {
    enabled: AtomicBool,
    frame: Mutex<AnalysisFrame>,
}
impl AudioAnalysis {
    pub(super) fn new(enabled: bool) -> Arc<Self> {
        Arc::new(Self {
            enabled: AtomicBool::new(enabled),
            frame: Mutex::new(AnalysisFrame::default()),
        })
    }
    pub(super) fn enable(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
    pub(super) fn snapshot(&self) -> Option<AnalysisFrame> {
        self.frame.try_lock().ok().map(|frame| *frame)
    }
}

pub(super) struct AnalysisSource<S> {
    source: S,
    tap: Arc<AudioAnalysis>,
    samples: [f32; WINDOW],
    real: [f32; WINDOW],
    imaginary: [f32; WINDOW],
    hann: [f32; WINDOW],
    twiddles: [(f32, f32); WINDOW / 2],
    count: usize,
    channel: u16,
    strongest: f32,
    frame_energy: f32,
    energy: f32,
    elapsed: f64,
    last_beat: f64,
    average: f32,
    previous_rms: f32,
    result: AnalysisFrame,
    collecting: bool,
}
impl<S: Source> AnalysisSource<S> {
    pub(super) fn new(source: S, tap: Arc<AudioAnalysis>) -> Self {
        Self {
            source,
            tap,
            samples: [0.0; WINDOW],
            real: [0.0; WINDOW],
            imaginary: [0.0; WINDOW],
            hann: std::array::from_fn(|i| 0.5 - 0.5 * (TAU * i as f32 / (WINDOW - 1) as f32).cos()),
            twiddles: std::array::from_fn(|i| {
                let angle = -TAU * i as f32 / WINDOW as f32;
                (angle.cos(), angle.sin())
            }),
            count: 0,
            channel: 0,
            strongest: 0.0,
            frame_energy: 0.0,
            energy: 0.0,
            elapsed: 0.0,
            last_beat: -1.0,
            average: 0.0,
            previous_rms: 0.0,
            result: AnalysisFrame::default(),
            collecting: false,
        }
    }
    fn reset(&mut self) {
        self.count = 0;
        self.channel = 0;
        self.strongest = 0.0;
        self.frame_energy = 0.0;
        self.energy = 0.0;
        self.average = 0.0;
        self.previous_rms = 0.0;
        self.result.rms = 0.0;
        self.result.bass = 0.0;
        self.result.treble = 0.0;
        self.result.bands.fill(0.0);
        self.result.wave.fill(0.0);
    }
    fn publish(&mut self) {
        self.result.sequence = self.result.sequence.wrapping_add(1);
        self.result.rms = (self.energy / WINDOW as f32).sqrt();
        self.elapsed += WINDOW as f64 / f64::from(self.source.sample_rate().get());
        for i in 0..WINDOW {
            self.real[i] = self.samples[i] * self.hann[i];
            self.imaginary[i] = 0.0;
        }
        // In-place radix-2 FFT; storage and trigonometry are prepared before playback.
        let mut j = 0;
        for i in 1..WINDOW {
            let mut bit = WINDOW >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                self.real.swap(i, j);
                self.imaginary.swap(i, j);
            }
        }
        let mut size = 2;
        while size <= WINDOW {
            for start in (0..WINDOW).step_by(size) {
                for offset in 0..size / 2 {
                    let (wr, wi) = self.twiddles[offset * WINDOW / size];
                    let a = start + offset;
                    let b = a + size / 2;
                    let re = wr * self.real[b] - wi * self.imaginary[b];
                    let im = wr * self.imaginary[b] + wi * self.real[b];
                    self.real[b] = self.real[a] - re;
                    self.imaginary[b] = self.imaginary[a] - im;
                    self.real[a] += re;
                    self.imaginary[a] += im;
                }
            }
            size *= 2;
        }
        let rate = self.source.sample_rate().get() as f32;
        let maximum = 16000.0f32.min(rate * 0.48);
        let minimum = 40.0f32.min(maximum / 2.0);
        let mut bass_energy = 0.0;
        let mut treble_energy = 0.0;
        for bin in 1..WINDOW / 2 {
            let frequency = bin as f32 * rate / WINDOW as f32;
            let power = (self.real[bin].powi(2) + self.imaginary[bin].powi(2))
                * (4.0 / (WINDOW * WINDOW) as f32);
            if frequency <= 250.0 {
                bass_energy += power;
            }
            if frequency >= 2500.0 {
                treble_energy += power;
            }
        }
        self.result.bass = bass_energy.sqrt();
        self.result.treble = treble_energy.sqrt();
        for band in 0..SPECTRUM_BANDS {
            let low = minimum * (maximum / minimum).powf(band as f32 / SPECTRUM_BANDS as f32);
            let high =
                minimum * (maximum / minimum).powf((band + 1) as f32 / SPECTRUM_BANDS as f32);
            let start = ((low * WINDOW as f32 / rate).round() as usize).clamp(1, WINDOW / 2 - 1);
            let end = ((high * WINDOW as f32 / rate).ceil() as usize).clamp(start + 1, WINDOW / 2);
            let power = (start..end)
                .map(|bin| self.real[bin].powi(2) + self.imaginary[bin].powi(2))
                .sum::<f32>();
            self.result.bands[band] = (power.sqrt() * 2.0 / WINDOW as f32).min(8.0);
        }
        for (i, sample) in self.result.wave.iter_mut().enumerate() {
            *sample = self.samples[i * WINDOW / WAVE_SAMPLES];
        }
        let rms = self.result.rms;
        if rms > 0.015
            && rms > self.average * 1.35 + 0.008
            && rms > self.previous_rms * 1.1 + 0.006
            && self.elapsed - self.last_beat >= 0.16
        {
            self.result.beats = self.result.beats.wrapping_add(1);
            self.last_beat = self.elapsed;
        }
        self.average += (rms - self.average) * (1.0 - (-(WINDOW as f32) / rate / 0.4).exp());
        self.previous_rms = rms;
        // Neither the callback nor the interface waits for the other thread.
        if let Ok(mut frame) = self.tap.frame.try_lock() {
            *frame = self.result;
        }
        self.count = 0;
        self.energy = 0.0;
    }
}
impl<S: Source> Iterator for AnalysisSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let sample = self.source.next()?;
        let enabled = self.tap.enabled.load(Ordering::Relaxed);
        if enabled != self.collecting {
            self.reset();
            self.collecting = enabled;
        }
        if enabled {
            let value = if sample.is_finite() {
                sample.clamp(-8.0, 8.0)
            } else {
                0.0
            };
            if value.abs() > self.strongest.abs() {
                self.strongest = value;
            }
            self.frame_energy += value * value;
            self.channel += 1;
            if self.channel >= self.source.channels().get() {
                // Select the strongest channel rather than cancelling antiphase stereo.
                self.samples[self.count] = self.strongest;
                self.energy += self.frame_energy / f32::from(self.channel);
                self.count += 1;
                self.channel = 0;
                self.strongest = 0.0;
                self.frame_energy = 0.0;
                if self.count == WINDOW {
                    self.publish();
                }
            }
        }
        Some(sample)
    }
}
impl<S: Source> Source for AnalysisSource<S> {
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
    fn try_seek(&mut self, position: Duration) -> Result<(), SeekError> {
        self.source.try_seek(position)?;
        self.reset();
        if let Ok(mut frame) = self.tap.frame.try_lock() {
            *frame = self.result;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn signal(frequency: f32, amplitude: f32, frames: usize, stereo: bool) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let value = (TAU * frequency * i as f32 / 48000.0).sin() * amplitude;
                if stereo {
                    vec![value, -value]
                } else {
                    vec![value]
                }
            })
            .collect()
    }
    fn source(
        samples: Vec<f32>,
        stereo: bool,
        tap: Arc<AudioAnalysis>,
    ) -> AnalysisSource<SamplesBuffer> {
        AnalysisSource::new(
            SamplesBuffer::new(
                ChannelCount::new(if stereo { 2 } else { 1 }).unwrap(),
                SampleRate::new(48000).unwrap(),
                samples,
            ),
            tap,
        )
    }
    #[test]
    fn tap_preserves_samples_and_detects_stereo_energy_frequency_and_attacks() {
        let tap = AudioAnalysis::new(true);
        let input = signal(93.75, 0.6, WINDOW * 8, true);
        assert_eq!(
            source(input.clone(), true, tap.clone()).collect::<Vec<_>>(),
            input
        );
        let low = tap.snapshot().unwrap();
        assert!((low.rms - 0.6 / 2.0f32.sqrt()).abs() < 0.01);
        assert!(low.bass > 0.3 && low.treble < 0.01);
        assert_eq!(low.beats, 1, "a held tone must not keep triggering beats");
        source(signal(6000.0, 0.6, WINDOW * 8, false), false, tap.clone()).for_each(drop);
        let high = tap.snapshot().unwrap();
        assert!(high.treble > 0.3 && high.bass < 0.01);
        let strongest = |frame: &AnalysisFrame| {
            frame
                .bands
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .unwrap()
                .0
        };
        assert!(strongest(&high) > strongest(&low) + 10);
        let mut pulses = Vec::new();
        for _ in 0..4 {
            pulses.extend(vec![0.0; WINDOW * 12]);
            pulses.extend(signal(93.75, 0.8, WINDOW * 3, false));
        }
        source(pulses, false, tap.clone()).for_each(drop);
        assert_eq!(tap.snapshot().unwrap().beats, 4);
    }
    #[test]
    fn silence_disabled_tap_and_contended_interface_do_not_block_or_invent_audio() {
        let tap = AudioAnalysis::new(false);
        source(signal(1000.0, 0.8, WINDOW * 2, false), false, tap.clone()).for_each(drop);
        assert_eq!(tap.snapshot().unwrap().sequence, 0);
        tap.enable(true);
        source(vec![0.0; WINDOW * 2], false, tap.clone()).for_each(drop);
        let frame = tap.snapshot().unwrap();
        assert_eq!(frame.rms, 0.0);
        assert_eq!(frame.beats, 0);
        assert!(frame.bands.iter().all(|band| *band == 0.0));
        let guard = tap.frame.lock().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let other = tap.clone();
        let worker = std::thread::spawn(move || {
            let count = source(signal(1000.0, 0.8, WINDOW * 4, false), false, other).count();
            sender.send(count).unwrap();
        });
        let result = receiver.recv_timeout(Duration::from_secs(1));
        drop(guard);
        worker.join().unwrap();
        assert_eq!(result.unwrap(), WINDOW * 4);
    }
}
