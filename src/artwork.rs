use crate::errors::AppError;
use anyhow::{Context, Result, ensure};
use image::ImageDecoder;
use lofty::{file::TaggedFileExt, picture::PictureType, probe::Probe};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read},
    path::PathBuf,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArtworkSettings {
    pub visible: bool,
    pub shared: bool,
    pub path: Option<PathBuf>,
}

impl Default for ArtworkSettings {
    fn default() -> Self {
        Self {
            visible: true,
            shared: false,
            path: None,
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

fn decode(bytes: &[u8]) -> Result<ArtworkImage> {
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

fn image_error(error: image::ImageError) -> anyhow::Error {
    if matches!(error, image::ImageError::Limits(_)) {
        anyhow::Error::new(AppError::ArtworkTooLarge)
    } else {
        anyhow::Error::new(error).context(AppError::InvalidArtwork)
    }
}
