//! Reverse frames from a private, automatically removed PCM cache.
use anyhow::{Context, Result, ensure};
use rodio::{
    ChannelCount, SampleRate, Source,
    source::{SeekError, UniformSourceIterator},
};
use std::{
    fs::File,
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const BLOCK_FRAMES: u64 = 1024;

pub(crate) struct PcmCache {
    file: Mutex<File>,
    frames: u64,
    channels: ChannelCount,
    rate: SampleRate,
}

impl PcmCache {
    pub(crate) fn decode(source: impl Source, cancel: &AtomicBool) -> Result<Arc<Self>> {
        let channels = source.channels();
        let rate = source.sample_rate();
        let source = UniformSourceIterator::new(source, channels, rate);
        let file = tempfile::tempfile().context("Cannot create reverse playback cache")?;
        let mut writer = BufWriter::with_capacity(65536, file);
        let mut samples = 0u64;
        for sample in source {
            if samples.is_multiple_of(4096) {
                ensure!(
                    !cancel.load(Ordering::Relaxed),
                    "Audio preparation canceled"
                );
            }
            writer
                .write_all(&sample.to_le_bytes())
                .context("Cannot write reverse playback cache")?;
            samples += 1;
        }
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Audio preparation canceled"
        );
        writer.flush()?;
        let file = writer.into_inner().map_err(|error| error.into_error())?;
        Ok(Arc::new(Self {
            file: Mutex::new(file),
            frames: samples / u64::from(channels.get()),
            channels,
            rate,
        }))
    }

    fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.frames as f64 / f64::from(self.rate.get()))
    }

    pub(crate) fn source(self: &Arc<Self>, position: Option<Duration>) -> ReverseSource {
        let frame = position
            .map_or(self.frames, |position| {
                (position.as_secs_f64() * f64::from(self.rate.get())).round() as u64
            })
            .min(self.frames);
        ReverseSource {
            cache: self.clone(),
            next_frame: frame,
            buffer: Vec::new(),
            index: 0,
            failed: false,
        }
    }
}

pub(crate) struct ReverseSource {
    pub(crate) cache: Arc<PcmCache>,
    next_frame: u64,
    buffer: Vec<f32>,
    index: usize,
    failed: bool,
}

impl ReverseSource {
    pub(crate) fn position(&self) -> Duration {
        Duration::from_secs_f64(self.next_frame as f64 / f64::from(self.cache.rate.get()))
    }

    fn refill(&mut self) -> std::io::Result<()> {
        let start = self.next_frame.saturating_sub(BLOCK_FRAMES);
        let channels = usize::from(self.cache.channels.get());
        let count = (self.next_frame - start) as usize * channels;
        let mut bytes = vec![0u8; count * 4];
        let mut file = self
            .cache
            .file
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        file.seek(SeekFrom::Start(start * channels as u64 * 4))?;
        file.read_exact(&mut bytes)?;
        drop(file);
        self.buffer.clear();
        self.buffer.reserve(count);
        // Preserve channel ordering inside each reversed audio frame.
        for frame in bytes.chunks_exact(channels * 4).rev() {
            for &sample in frame.as_chunks::<4>().0 {
                self.buffer.push(f32::from_le_bytes(sample));
            }
        }
        self.next_frame = start;
        self.index = 0;
        Ok(())
    }
}

impl Iterator for ReverseSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.index == self.buffer.len() {
            if self.next_frame == 0 || self.failed {
                return None;
            }
            if self.refill().is_err() {
                self.failed = true;
                return None;
            }
        }
        let sample = self.buffer[self.index];
        self.index += 1;
        Some(sample)
    }
}

impl Source for ReverseSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        self.cache.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.cache.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        Some(self.cache.duration())
    }
    fn try_seek(&mut self, position: Duration) -> std::result::Result<(), SeekError> {
        self.next_frame =
            (position.as_secs_f64() * f64::from(self.cache.rate.get())).round() as u64;
        self.next_frame = self.next_frame.min(self.cache.frames);
        self.buffer.clear();
        self.index = 0;
        self.failed = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;
    #[test]
    fn reverses_frames_across_blocks_without_swapping_channels_and_seeks_in_original_time() {
        let samples: Vec<_> = (0..2400)
            .flat_map(|frame| [frame as f32, -(frame as f32)])
            .collect();
        let source = SamplesBuffer::new(
            ChannelCount::new(2).unwrap(),
            SampleRate::new(1000).unwrap(),
            samples,
        );
        let cache = PcmCache::decode(source, &AtomicBool::new(false)).unwrap();
        let mut reverse = cache.source(None);
        assert_eq!(reverse.total_duration(), Some(Duration::from_millis(2400)));
        assert_eq!(reverse.position(), Duration::from_millis(2400));
        let expected: Vec<_> = (0..2400)
            .rev()
            .flat_map(|frame| [frame as f32, -(frame as f32)])
            .collect();
        assert_eq!(reverse.by_ref().collect::<Vec<_>>(), expected);
        reverse.try_seek(Duration::from_millis(1200)).unwrap();
        assert_eq!(
            reverse.take(4).collect::<Vec<_>>(),
            vec![1199.0, -1199.0, 1198.0, -1198.0]
        );
        assert_eq!(cache.source(Some(Duration::ZERO)).count(), 0);
    }
    #[test]
    fn cancellation_and_empty_input_release_the_private_cache() {
        let source = SamplesBuffer::new(
            ChannelCount::new(1).unwrap(),
            SampleRate::new(8000).unwrap(),
            vec![0.0; 9000],
        );
        assert!(PcmCache::decode(source, &AtomicBool::new(true)).is_err());
        let source = SamplesBuffer::new(
            ChannelCount::new(1).unwrap(),
            SampleRate::new(8000).unwrap(),
            vec![],
        );
        let cache = PcmCache::decode(source, &AtomicBool::new(false)).unwrap();
        assert_eq!(cache.source(None).count(), 0);
    }
}
