use crate::errors::AppError;
use crate::model::ImportedTrack;
use anyhow::{Context, Result};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    probe::Probe,
    tag::Accessor,
};
use rodio::{Decoder, Source};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "mp3" | "flac" | "wav" | "ogg" | "oga" | "m4a" | "aac" | "aiff" | "aif"
            )
        })
}

pub fn read_track(path: &Path) -> Result<ImportedTrack> {
    let path = path
        .canonicalize()
        .with_context(|| format!("File unavailable: {}", path.display()))?;
    anyhow::ensure!(path.is_file(), "Not a file: {}", path.display());
    anyhow::ensure!(path.to_str().is_some(), AppError::InvalidPathEncoding);
    let decoder = Decoder::try_from(std::fs::File::open(&path)?)
        .with_context(|| format!("Unsupported or damaged audio: {}", path.display()))?;
    let decoded_duration = decoder.total_duration().map_or(0, |duration| {
        duration.as_millis().min(u64::MAX as u128) as u64
    });
    let filename = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut result = ImportedTrack {
        path: path.clone(),
        title: filename,
        artist: String::new(),
        album: String::new(),
        duration_ms: decoded_duration,
        tags: "{}".into(),
    };
    // Чтение тегов не открывает музыкальные файлы на запись.
    if let Ok(file) = Probe::open(&path).and_then(|probe| probe.read()) {
        let duration = file
            .properties()
            .duration()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        if duration > 0 {
            result.duration_ms = duration;
        }
        if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
            if let Some(title) = tag.title().filter(|value| !value.is_empty()) {
                result.title = title.into_owned();
            }
            result.artist = tag
                .artist()
                .map(|value| value.into_owned())
                .unwrap_or_default();
            result.album = tag
                .album()
                .map(|value| value.into_owned())
                .unwrap_or_default();
        }
        let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for tag in file.tags() {
            for item in tag.items() {
                let key = format!("{:?}.{:?}", tag.tag_type(), item.key());
                tags.entry(key)
                    .or_default()
                    .push(format!("{:?}", item.value()));
            }
        }
        result.tags = serde_json::to_string(&tags)?;
    } else {
        result.tags = serde_json::to_string(&BTreeMap::from([(
            "MetadataNote",
            vec!["Tag reader could not extract metadata; the source file is unchanged."],
        )]))?;
    }
    Ok(result)
}

pub fn audio_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![directory.canonicalize()?];
    while let Some(directory) = directories.pop() {
        let mut entries = std::fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            // Символические ссылки на каталоги не обходятся: исключены циклы.
            let kind = entry.file_type()?;
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file() && is_audio(&entry.path()) {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}
