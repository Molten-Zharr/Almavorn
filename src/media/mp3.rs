//! Count MPEG audio packets instead of extrapolating a variable bitrate.
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::ErrorKind,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use symphonia::core::{
    codecs::CODEC_TYPE_MP3, errors::Error, formats::FormatOptions, io::MediaSourceStream,
    probe::Hint,
};

pub(crate) fn mp3_duration(path: &Path, cancel: Option<&AtomicBool>) -> Result<Option<Duration>> {
    // An MP3 codec inside MP4/WAV is not an elementary MPEG stream: its packet
    // timestamps and seek requirements belong to the enclosing container.
    if lofty::probe::Probe::open(path)?.guess_file_type()?.file_type()
        != Some(lofty::file::FileType::Mpeg)
    {
        return Ok(None);
    }
    let input = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    // Count untrimmed packets, then apply the real encoder delay/padding once.
    // Gapless trimming based on an estimated n_frames could truncate the count.
    let mut format = symphonia::default::get_probe()
        .format(
            &Hint::new(),
            input,
            &FormatOptions::default(),
            &Default::default(),
        )
        .context("Cannot read audio stream timing")?
        .format;
    let Some(track) = format.default_track() else {
        return Ok(None);
    };
    if track.codec_params.codec != CODEC_TYPE_MP3 {
        return Ok(None);
    }
    let id = track.id;
    let rate = track
        .codec_params
        .sample_rate
        .context("MP3 sample rate is missing")?;
    ensure!(rate > 0, "Invalid MP3 sample rate");
    let trim = u64::from(track.codec_params.delay.unwrap_or(0))
        + u64::from(track.codec_params.padding.unwrap_or(0));
    let mut samples = 0u64;
    loop {
        ensure!(
            !cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)),
            "Audio preparation canceled"
        );
        match format.next_packet() {
            Ok(packet) if packet.track_id() == id => {
                samples = samples
                    .checked_add(packet.dur)
                    .context("MP3 duration overflow")?;
            }
            Ok(_) => {}
            Err(Error::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error).context("Cannot count MP3 audio frames"),
        }
    }
    ensure!(samples > trim, "MP3 contains no complete audio frames");
    let samples = samples - trim;
    let rate = u64::from(rate);
    Ok(Some(Duration::new(
        samples / rate,
        ((samples % rate) * 1_000_000_000 / rate) as u32,
    )))
}
