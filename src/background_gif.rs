use crate::{artwork::ArtworkImage, model::Language};
use anyhow::{Context, Result, ensure};
use image::{AnimationDecoder, ImageDecoder};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GifBackgroundSettings {
    pub path: Option<PathBuf>,
    pub opacity: u8,
}

impl Default for GifBackgroundSettings {
    fn default() -> Self {
        Self {
            path: None,
            opacity: 15,
        }
    }
}

pub struct GifAnimation {
    pub frames: Vec<Arc<ArtworkImage>>,
    ends_ms: Vec<u64>,
    duration_ms: u64,
}

impl GifAnimation {
    pub fn frame_at(&self, elapsed: std::time::Duration) -> usize {
        let position = (elapsed.as_millis() % u128::from(self.duration_ms)) as u64;
        self.ends_ms
            .partition_point(|&end| end <= position)
            .min(self.frames.len() - 1)
    }
}

pub(crate) fn read(path: &Path, language: Language) -> Result<GifAnimation> {
    let file = std::fs::File::open(path).with_context(|| {
        language.text(
            "Cannot open the background GIF",
            "Не удалось открыть GIF-фон",
        )
    })?;
    ensure!(
        file.metadata()?.is_file(),
        language.text("Choose a GIF file", "Выберите GIF-файл")
    );
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        language.text(
            "GIF exceeds 32 MB. Choose a smaller file.",
            "GIF больше 32 МБ. Выберите файл поменьше."
        )
    );
    let mut decoder =
        image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).context(language.text(
            "The GIF file is damaged or unsupported",
            "GIF-файл поврежден или не поддерживается",
        ))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    decoder.set_limits(limits)?;
    let (width, height) = decoder.dimensions();
    ensure!(
        width > 0 && height > 0,
        language.text("GIF has no image", "В GIF нет изображения")
    );
    let mut frames = Vec::new();
    let mut ends_ms = Vec::new();
    let mut duration_ms = 0u64;
    let mut memory = 0usize;
    for frame in decoder.into_frames() {
        ensure!(frames.len() < 600 && (frames.len() as u64 + 1) * u64::from(width) * u64::from(height) <= 200_000_000,
            language.text("GIF is too long or too large. Choose a shorter or smaller animation.", "GIF слишком длинный или большой. Выберите анимацию покороче или с меньшим разрешением."));
        let frame = frame
            .context(language.text("Cannot decode a GIF frame", "Не удалось прочитать кадр GIF"))?;
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let delay = (u64::from(numerator) / u64::from(denominator.max(1))).max(20);
        let pixels = image::DynamicImage::ImageRgba8(frame.into_buffer())
            .thumbnail(960, 540)
            .into_rgba8();
        memory += pixels.len();
        ensure!(memory <= 128 * 1024 * 1024, language.text("GIF needs too much memory. Choose a shorter or smaller animation.", "GIF требует слишком много памяти. Выберите анимацию покороче или с меньшим разрешением."));
        duration_ms = duration_ms.saturating_add(delay);
        ends_ms.push(duration_ms);
        frames.push(Arc::new(ArtworkImage {
            width: pixels.width(),
            height: pixels.height(),
            rgba: pixels.into_raw(),
        }));
    }
    ensure!(
        !frames.is_empty(),
        language.text("GIF contains no frames", "В GIF нет кадров")
    );
    Ok(GifAnimation {
        frames,
        ends_ms,
        duration_ms,
    })
}
