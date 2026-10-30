use crate::{errors::AppError, model::Language};
use anyhow::{Context, Result, ensure};
use image::ImageDecoder;
use lofty::{file::TaggedFileExt, picture::PictureType, probe::Probe};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

pub(crate) mod search;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtworkSettings {
    pub visible: bool,
    pub shared: bool,
    pub path: Option<PathBuf>,
    pub size: ArtworkSize,
    pub search_online: bool,
}

impl Default for ArtworkSettings {
    fn default() -> Self {
        Self {
            visible: true,
            shared: false,
            path: None,
            size: ArtworkSize::default(),
            search_online: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkSize {
    Mini,
    Small,
    #[default]
    Medium,
    Large,
}

impl ArtworkSize {
    pub const ALL: [Self; 4] = [Self::Mini, Self::Small, Self::Medium, Self::Large];

    pub fn columns(self) -> u16 {
        match self {
            Self::Mini => 4,
            Self::Small => 8,
            Self::Medium => 16,
            Self::Large => 24,
        }
    }

    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Mini => language.text("Mini", "Мини"),
            Self::Small => language.text("Small", "Маленький"),
            Self::Medium => language.text("Medium", "Средний"),
            Self::Large => language.text("Large", "Большой"),
        }
    }
}

pub struct ArtworkImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArtworkRequest {
    Embedded(PathBuf),
    Shared(PathBuf),
}

pub(crate) fn read(request: &ArtworkRequest) -> Result<Option<ArtworkImage>> {
    match request {
        ArtworkRequest::Shared(path) => {
            let file = std::fs::File::open(path).context(AppError::InvalidArtwork)?;
            ensure!(file.metadata()?.is_file(), AppError::InvalidArtwork);
            let mut bytes = Vec::new();
            file.take(32 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .context(AppError::InvalidArtwork)?;
            decode(&bytes).map(Some)
        }
        ArtworkRequest::Embedded(path) => {
            let file = Probe::open(path)
                .and_then(|probe| probe.read())
                .context(AppError::InvalidArtwork)?;
            let picture = file
                .tags()
                .iter()
                .flat_map(|tag| tag.pictures())
                .min_by_key(|picture| picture.pic_type() != PictureType::CoverFront);
            picture.map(|picture| decode(picture.data())).transpose()
        }
    }
}

pub(crate) fn original_path(request: &ArtworkRequest, cache: &Path) -> Result<PathBuf> {
    let source = match request {
        ArtworkRequest::Shared(path) => {
            ensure!(path.is_file(), AppError::InvalidArtwork);
            return path.canonicalize().context(AppError::InvalidArtwork);
        }
        ArtworkRequest::Embedded(path) => path,
    };
    let file = Probe::open(source)
        .and_then(|probe| probe.read())
        .context(AppError::InvalidArtwork)?;
    let picture = file
        .tags()
        .iter()
        .flat_map(|tag| tag.pictures())
        .min_by_key(|picture| picture.pic_type() != PictureType::CoverFront)
        .context(AppError::InvalidArtwork)?;
    let bytes = picture.data();
    ensure!(bytes.len() <= 32 * 1024 * 1024, AppError::ArtworkTooLarge);
    let extension = match image::guess_format(bytes).context(AppError::InvalidArtwork)? {
        image::ImageFormat::Png => "png",
        image::ImageFormat::Jpeg => "jpg",
        _ => return Err(AppError::InvalidArtwork.into()),
    };
    let directory = cache.join(format!("{:x}", Sha256::digest(bytes)));
    let mut name = source.file_stem().unwrap_or(std::ffi::OsStr::new("cover")).to_os_string();
    name.push(format!(".{extension}"));
    let path = directory.join(name);
    if path.is_file() {
        return Ok(path);
    }
    std::fs::create_dir_all(&directory).context(AppError::ArtworkExportFailed)?;
    let mut file = tempfile::NamedTempFile::new_in(&directory).context(AppError::ArtworkExportFailed)?;
    file.write_all(bytes).context(AppError::ArtworkExportFailed)?;
    match file.persist_noclobber(&path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error).context(AppError::ArtworkExportFailed),
    }
    Ok(path)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<ArtworkImage> {
    ensure!(bytes.len() <= 32 * 1024 * 1024, AppError::ArtworkTooLarge);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context(AppError::InvalidArtwork)?;
    ensure!(
        matches!(
            reader.format(),
            Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        ),
        AppError::InvalidArtwork
    );
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    ensure!(
        decoder.total_bytes() <= 256 * 1024 * 1024,
        AppError::ArtworkTooLarge
    );
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(image_error)?;
    decoded.apply_orientation(orientation);
    let thumbnail = decoded.thumbnail(512, 512).into_rgba8();
    Ok(ArtworkImage {
        width: thumbnail.width(),
        height: thumbnail.height(),
        rgba: thumbnail.into_raw(),
    })
}

pub(crate) fn validate_bytes(bytes: &[u8]) -> Result<()> {
    decode(bytes).map(|_| ())
}

fn image_error(error: image::ImageError) -> anyhow::Error {
    if matches!(error, image::ImageError::Limits(_)) {
        anyhow::Error::new(AppError::ArtworkTooLarge)
    } else {
        anyhow::Error::new(error).context(AppError::InvalidArtwork)
    }
}
