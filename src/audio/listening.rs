use rodio::{ChannelCount, SampleRate, Source, source::SeekError};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

/// Count rendered PCM time, independently of the media position, FX speed,
/// pause duration and whether the visualizer is enabled. Never waits or allocates.
pub(super) struct ListeningSource<S: Source> {
    source: S,
    clock: Arc<AtomicU64>,
    channel: u16,
    frames: u64,
    fraction: u64,
    rate: u64,
}
impl<S: Source> ListeningSource<S> {
    pub(super) fn new(source: S, clock: Arc<AtomicU64>) -> Self {
        let rate = u64::from(source.sample_rate().get());
        Self {
            source,
            clock,
            channel: 0,
            frames: 0,
            fraction: 0,
            rate,
        }
    }
    fn publish(&mut self) {
        let numerator = self.frames * 1_000_000_000 + self.fraction;
        self.clock
            .fetch_add(numerator / self.rate, Ordering::Relaxed);
        self.fraction = numerator % self.rate;
        self.frames = 0;
    }
}
impl<S: Source> Iterator for ListeningSource<S> {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let Some(sample) = self.source.next() else {
            self.publish();
            return None;
        };
        self.channel += 1;
        if self.channel >= self.source.channels().get() {
            self.channel = 0;
            let rate = u64::from(self.source.sample_rate().get());
            if rate != self.rate {
                self.publish();
                self.rate = rate;
                self.fraction = 0;
            }
            self.frames += 1;
            if self.frames == 64 {
                self.publish();
            }
        }
        Some(sample)
    }
}
impl<S: Source> Source for ListeningSource<S> {
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
        self.publish();
        self.source.try_seek(position)?;
        self.channel = 0;
        Ok(())
    }
}
impl<S: Source> Drop for ListeningSource<S> {
    fn drop(&mut self) {
        self.publish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    fn source(channels: u16, rate: u32, frames: usize) -> SamplesBuffer {
        SamplesBuffer::new(
            ChannelCount::new(channels).unwrap(),
            SampleRate::new(rate).unwrap(),
            vec![0.25; frames * usize::from(channels)],
        )
    }
    #[test]
    fn counts_frames_instead_of_interleaved_samples_and_retains_fractional_time() {
        for rate in [44100, 48000] {
            for channels in [1, 2, 6] {
                let clock = Arc::new(AtomicU64::new(0));
                let samples =
                    ListeningSource::new(source(channels, rate, rate as usize), clock.clone())
                        .collect::<Vec<_>>();
                assert!(samples.iter().all(|sample| *sample == 0.25));
                assert_eq!(clock.load(Ordering::Relaxed), 1_000_000_000);
            }
        }
    }
    #[test]
    fn pauses_seeks_and_source_replacements_do_not_add_unheard_media_time() {
        let clock = Arc::new(AtomicU64::new(0));
        let mut stream =
            ListeningSource::new(source(2, 48000, 480000), clock.clone()).pausable(true);
        for _ in 0..96000 {
            assert_eq!(stream.next(), Some(0.0));
        }
        assert_eq!(clock.load(Ordering::Relaxed), 0);
        stream.set_paused(false);
        for _ in 0..96000 {
            stream.next();
        }
        assert_eq!(clock.load(Ordering::Relaxed), 1_000_000_000);
        stream.try_seek(Duration::from_secs(8)).unwrap();
        assert_eq!(clock.load(Ordering::Relaxed), 1_000_000_000);
        for _ in 0..9600 {
            stream.next();
        }
        drop(stream);
        assert_eq!(clock.load(Ordering::Relaxed), 1_100_000_000);
        ListeningSource::new(source(1, 48000, 48000), clock.clone()).for_each(drop);
        assert_eq!(clock.load(Ordering::Relaxed), 2_100_000_000);
    }
}
