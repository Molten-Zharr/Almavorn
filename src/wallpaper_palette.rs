use crate::{
    model::Language,
    preferences::{NamedPalette, default_palettes},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    #[default]
    Palette,
    Desktop,
    File,
}
impl Source {
    pub const ALL: [Self; 3] = [Self::Palette, Self::Desktop, Self::File];
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Palette => lang.text("Selected palette", "Выбранная палитра"),
            Self::Desktop => lang.text("Noctalia wallpaper", "Обои Noctalia"),
            Self::File => lang.text("Image file", "Изображение из файла"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scheme {
    #[default]
    Faithful,
    Vibrant,
    Soft,
    Monochrome,
    Inverted,
}
impl Scheme {
    pub const ALL: [Self; 5] = [
        Self::Faithful,
        Self::Vibrant,
        Self::Soft,
        Self::Monochrome,
        Self::Inverted,
    ];
    pub fn name(self, lang: Language) -> &'static str {
        match self {
            Self::Faithful => lang.text("Image colors", "Цвета изображения"),
            Self::Vibrant => lang.text("Vibrant", "Яркая"),
            Self::Soft => lang.text("Soft", "Мягкая"),
            Self::Monochrome => lang.text("Monochrome", "Монохромная"),
            Self::Inverted => lang.text("Inverted colors", "Инверсия цветов"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallpaperSettings {
    pub source: Source,
    pub path: Option<PathBuf>,
    pub scheme: Scheme,
    pub light: bool,
    pub pure_black: bool,
}
impl Default for WallpaperSettings {
    fn default() -> Self {
        Self {
            source: Source::Palette,
            path: None,
            scheme: Scheme::Faithful,
            light: false,
            pure_black: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageStamp {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
}

fn wallpaper_path(language: Language) -> Result<PathBuf> {
    let mut child = Command::new("noctalia")
        .args(["msg", "wallpaper-get"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context(language.text(
            "Cannot get wallpaper from Noctalia. Choose an image instead.",
            "Не удалось получить обои из Noctalia. Выберите изображение вручную.",
        ))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(language.text(
                "Noctalia did not respond. Choose an image instead.",
                "Noctalia не ответила. Выберите изображение вручную."
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success() && output.stdout.len() <= 8192,
        language.text(
            "Noctalia did not return a wallpaper path.",
            "Noctalia не вернула путь к обоям."
        )
    );
    let path = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(
        !path.is_empty(),
        language.text(
            "Noctalia did not return a wallpaper path.",
            "Noctalia не вернула путь к обоям."
        )
    );
    Ok(PathBuf::from(path))
}

fn mix(a: [u8; 3], b: [u8; 3], amount: f32) -> [u8; 3] {
    std::array::from_fn(|i| {
        (f32::from(a[i]) * (1.0 - amount) + f32::from(b[i]) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    })
}
fn luminance(color: [u8; 3]) -> f32 {
    let c = color.map(|v| {
        let x = f32::from(v) / 255.0;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}
fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}
fn legible(color: [u8; 3], backgrounds: &[[u8; 3]], light: bool) -> [u8; 3] {
    let target = if light { [0, 0, 0] } else { [255, 255, 255] };
    (0..=100)
        .map(|n| mix(color, target, n as f32 / 100.0))
        .find(|candidate| {
            backgrounds
                .iter()
                .all(|background| contrast(*candidate, *background) >= 4.5)
        })
        .unwrap_or(target)
}

fn dominant(path: &Path, language: Language) -> Result<[u8; 3]> {
    let file = std::fs::File::open(path).context(language.text(
        "Cannot open the image.",
        "Не удалось открыть изображение обоев.",
    ))?;
    ensure!(
        file.metadata()?.len() <= 64 * 1024 * 1024,
        language.text("The image exceeds 64 MB.", "Изображение больше 64 МБ.")
    );
    let mut reader =
        image::ImageReader::new(std::io::BufReader::new(file)).with_guessed_format()?;
    ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::Gif)
        ),
        language.text(
            "PNG, JPEG and GIF are supported.",
            "Поддерживаются PNG, JPEG и GIF."
        )
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let pixels = reader
        .decode()
        .context(language.text(
            "Cannot read the image.",
            "Не удалось прочитать изображение обоев.",
        ))?
        .thumbnail(128, 128)
        .to_rgba8();
    let mut bins: HashMap<(u8, u8, u8), (u64, [u64; 3])> = HashMap::new();
    for pixel in pixels.pixels().filter(|pixel| pixel[3] >= 128) {
        let key = (pixel[0] >> 4, pixel[1] >> 4, pixel[2] >> 4);
        let entry = bins.entry(key).or_insert((0, [0; 3]));
        entry.0 += 1;
        for (i, value) in entry.1.iter_mut().enumerate() {
            *value += u64::from(pixel[i]);
        }
    }
    let (_, (count, sums)) = bins
        .into_iter()
        .max_by(|(a, (ca, _)), (b, (cb, _))| {
            let score = |key: &(u8, u8, u8), count: u64| {
                let max = key.0.max(key.1).max(key.2);
                let min = key.0.min(key.1).min(key.2);
                count * u64::from(8 + max - min) * if max < 2 || min > 13 { 1 } else { 3 }
            };
            score(a, *ca).cmp(&score(b, *cb)).then_with(|| a.cmp(b))
        })
        .context(language.text(
            "The image has no visible colors.",
            "В изображении нет видимых цветов.",
        ))?;
    Ok(sums.map(|sum| (sum / count) as u8))
}

fn generate(seed: [u8; 3], settings: &WallpaperSettings) -> NamedPalette {
    let gray = (0.2126 * f32::from(seed[0])
        + 0.7152 * f32::from(seed[1])
        + 0.0722 * f32::from(seed[2])) as u8;
    let seed = match settings.scheme {
        Scheme::Faithful => seed,
        Scheme::Vibrant => {
            let max = *seed.iter().max().unwrap();
            let min = *seed.iter().min().unwrap();
            if max == min {
                seed
            } else {
                seed.map(|v| (((v - min) as f32 / (max - min) as f32) * 255.0) as u8)
            }
        }
        Scheme::Soft => mix(seed, [gray; 3], 0.55),
        Scheme::Monochrome => [gray; 3],
        Scheme::Inverted => seed.map(|v| 255 - v),
    };
    let light = settings.light;
    let background = if light {
        mix(seed, [255; 3], 0.94)
    } else if settings.pure_black {
        [0; 3]
    } else {
        mix(seed, [0; 3], 0.91)
    };
    let selected = mix(
        background,
        if light { [0; 3] } else { [255; 3] },
        if light { 0.12 } else { 0.18 },
    );
    let accent = legible(seed, &[background, selected], light);
    let text = legible(
        if light { [35; 3] } else { [235; 3] },
        &[background, selected],
        light,
    );
    let mut palette = default_palettes().remove(0);
    palette.name = "Wallpaper".into();
    palette.id = "wallpaper-preview".into();
    for (key, color) in [
        ("background", background),
        ("selected_file_background", selected),
        ("text", text),
        ("button_text", text),
        ("active", accent),
        ("sidebar_title", accent),
        (
            "error",
            legible(
                if light {
                    [185, 20, 35]
                } else {
                    [255, 100, 100]
                },
                &[background],
                light,
            ),
        ),
        ("inactive_file_border", mix(accent, background, 0.35)),
        ("inactive_panel_border", mix(accent, background, 0.45)),
        ("gradient_dark", mix(accent, background, 0.55)),
        ("folder_icon", accent),
        ("selection_accent", accent),
        ("confirm_background", selected),
        ("help_heading", accent),
        (
            "folder_path",
            legible(mix(accent, text, 0.35), &[background], light),
        ),
        ("separator", mix(accent, background, 0.5)),
    ] {
        palette.colors.insert(key.into(), color);
    }
    palette
}

pub(crate) fn refresh(
    settings: &WallpaperSettings,
    previous: Option<&ImageStamp>,
    language: Language,
) -> Result<(ImageStamp, Option<NamedPalette>)> {
    let path = match settings.source {
        Source::Palette => anyhow::bail!(language.text(
            "Wallpaper colors are not selected.",
            "Палитра обоев не выбрана."
        )),
        Source::Desktop => wallpaper_path(language)?,
        Source::File => settings
            .path
            .clone()
            .context(language.text("Choose an image.", "Выберите изображение."))?,
    };
    let metadata = std::fs::metadata(&path).context(language.text(
        "The wallpaper image is unavailable.",
        "Изображение обоев недоступно.",
    ))?;
    ensure!(
        metadata.is_file(),
        language.text("Choose an image file.", "Выберите файл изображения.")
    );
    let stamp = ImageStamp {
        path: path.clone(),
        modified: metadata.modified().ok(),
        length: metadata.len(),
    };
    if previous == Some(&stamp) {
        return Ok((stamp, None));
    }
    let seed = dominant(&path, language)?;
    let after = std::fs::metadata(&path)?;
    ensure!(
        after.len() == stamp.length && after.modified().ok() == stamp.modified,
        language.text(
            "The image changed while it was read.",
            "Изображение изменилось во время чтения."
        )
    );
    Ok((stamp, Some(generate(seed, settings))))
}
