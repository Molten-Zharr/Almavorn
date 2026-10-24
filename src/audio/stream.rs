use crate::services::Media;
use anyhow::{Context, Result, ensure};
use rodio::{ChannelCount, Decoder, SampleRate, Source};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError, TrySendError},
    },
    time::Duration,
};

#[derive(Clone)]
pub struct StreamStatus {
    pub error: Arc<Mutex<Option<String>>>,
    pub silence: Arc<AtomicU64>,
    pub samples_per_second: u64,
}
impl StreamStatus {
    pub fn buffering_ms(&self) -> u64 {
        self.silence.load(Ordering::Relaxed) * 1000 / self.samples_per_second.max(1)
    }
}
struct Reader {
    client: reqwest::blocking::Client,
    media: Media,
    position: u64,
    length: Option<u64>,
    start: u64,
    bytes: Vec<u8>,
    cancel: Arc<AtomicBool>,
    error: Arc<Mutex<Option<String>>>,
}
impl Reader {
    fn fetch(&mut self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Streaming cancelled",
            ));
        }
        let mut request = self.client.get(&self.media.url).header(
            "Range",
            format!("bytes={}-{}", self.position, self.position + 512 * 1024 - 1),
        );
        for (k, v) in &self.media.headers {
            if !k.eq_ignore_ascii_case("range") {
                request = request.header(k, v);
            }
        }
        let response = request
            .send()
            .map_err(|_| io::Error::other("Audio connection failed"))?;
        if response.status().as_u16() == 416 {
            self.bytes.clear();
            self.length = Some(self.position);
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(io::Error::other("Audio access expired or unavailable"));
        }
        if response.status().as_u16() == 206 {
            let range = response
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| io::Error::other("Missing audio range"))?;
            let (start, length) = range
                .strip_prefix("bytes ")
                .and_then(|r| r.split_once('/'))
                .ok_or_else(|| io::Error::other("Invalid audio range"))?;
            let start = start.split('-').next().and_then(|s| s.parse::<u64>().ok());
            if start != Some(self.position) {
                return Err(io::Error::other("Unexpected audio range"));
            }
            self.length = length.parse::<u64>().ok();
        } else if self.position != 0 {
            return Err(io::Error::other(
                "This service does not support audio seeking",
            ));
        } else {
            self.length = response.content_length();
        }
        self.start = self.position;
        self.bytes.clear();
        response.take(512 * 1024 + 1).read_to_end(&mut self.bytes)?;
        if self.bytes.len() > 512 * 1024 {
            return Err(io::Error::other(
                "This service does not support ranged audio",
            ));
        }
        if self.bytes.is_empty() {
            self.length = Some(self.position);
        }
        Ok(())
    }
}
impl Read for Reader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.length.is_some_and(|n| self.position >= n) {
            return Ok(0);
        }
        if self.position < self.start || self.position >= self.start + self.bytes.len() as u64 {
            if let Err(error) = self.fetch() {
                if !self.cancel.load(Ordering::Relaxed)
                    && let Ok(mut message) = self.error.lock()
                {
                    *message = Some("Передача аудио прервалась. Повторите воспроизведение.".into());
                }
                return Err(error);
            }
        }
        let offset = self.position.saturating_sub(self.start) as usize;
        let count = output.len().min(self.bytes.len().saturating_sub(offset));
        if count > 0 {
            output[..count].copy_from_slice(&self.bytes[offset..offset + count]);
            self.position += count as u64;
        }
        Ok(count)
    }
}
impl Seek for Reader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let value = match position {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::Current(n) => i128::from(self.position) + i128::from(n),
            SeekFrom::End(n) => {
                i128::from(
                    self.length
                        .ok_or_else(|| io::Error::other("Unknown audio length"))?,
                ) + i128::from(n)
            }
        };
        if value < 0 || value > u64::MAX as i128 {
            return Err(io::Error::other("Invalid audio position"));
        }
        self.position = value as u64;
        Ok(self.position)
    }
}
struct Buffered {
    receiver: Receiver<Vec<f32>>,
    current: std::vec::IntoIter<f32>,
    channels: ChannelCount,
    rate: SampleRate,
    duration: Option<Duration>,
    cancel: Arc<AtomicBool>,
    status: StreamStatus,
}
impl Drop for Buffered {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl Iterator for Buffered {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if let Some(sample) = self.current.next() {
            return Some(sample);
        }
        match self.receiver.try_recv() {
            Ok(samples) => {
                self.current = samples.into_iter();
                self.current.next()
            }
            Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                self.status.silence.fetch_add(1, Ordering::Relaxed);
                Some(0.)
            }
        }
    }
}
impl Source for Buffered {
    fn current_span_len(&self) -> Option<usize> {
        Some(1024)
    }
    fn channels(&self) -> ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
}

pub(super) fn prepare(
    media: Media,
    position: Duration,
    cancel: Arc<AtomicBool>,
) -> Result<(super::PreparedAudio, StreamStatus)> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()?;
    let error = Arc::new(Mutex::new(None));
    let mut reader = Reader {
        client,
        media,
        position: 0,
        length: None,
        start: 0,
        bytes: vec![],
        cancel: cancel.clone(),
        error: error.clone(),
    };
    reader
        .fetch()
        .context("Не удалось получить аудио. Повторите воспроизведение.")?;
    let length = reader.length;
    let mut builder = Decoder::builder().with_data(reader).with_seekable(true);
    if let Some(length) = length {
        builder = builder.with_byte_len(length);
    }
    let mut decoder = builder
        .build()
        .context("Формат аудио сервиса не поддерживается.")?;
    if !position.is_zero() {
        decoder
            .try_seek(position)
            .context("Не удалось перейти к выбранному месту композиции.")?;
    }
    let channels = decoder.channels();
    let rate = decoder.sample_rate();
    let duration = decoder.total_duration();
    let status = StreamStatus {
        error,
        silence: Arc::new(AtomicU64::new(0)),
        samples_per_second: u64::from(channels.get()) * u64::from(rate.get()),
    };
    let first = decoder.by_ref().take(32768).collect::<Vec<_>>();
    ensure!(!first.is_empty(), "Сервис вернул пустое аудио.");
    let (sender, receiver) = mpsc::sync_channel(32);
    let worker_cancel = cancel.clone();
    let errors = status.error.clone();
    std::thread::Builder::new()
        .name("almavorn-stream".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while !worker_cancel.load(Ordering::Relaxed) {
                    let mut samples = decoder.by_ref().take(16384).collect::<Vec<_>>();
                    if samples.is_empty() {
                        break;
                    }
                    loop {
                        if worker_cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        match sender.try_send(samples) {
                            Ok(()) => break,
                            Err(TrySendError::Full(value)) => {
                                samples = value;
                                std::thread::sleep(Duration::from_millis(10));
                            }
                            Err(TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
            }));
            if result.is_err()
                && let Ok(mut error) = errors.lock()
            {
                *error = Some("Передача аудио прервалась. Повторите воспроизведение.".into());
            }
        })?;
    let source = Buffered {
        receiver,
        current: first.into_iter(),
        channels,
        rate,
        duration,
        cancel,
        status: status.clone(),
    };
    Ok((
        super::PreparedAudio {
            source: Box::new(source),
            cache: None,
            position,
            reverse: false,
        },
        status,
    ))
}
