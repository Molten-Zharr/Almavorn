use super::{
    FileSnapshot, NoCover, SearchProgress, manual_search, normalized, preview, save_manual, usable,
};
use crate::{
    model::{Language, Track},
    tags::Document,
};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

#[derive(Clone, Copy, Default)]
pub(crate) enum Stage {
    #[default]
    Checking,
    Searching,
    Loading {
        edition: usize,
        total: usize,
    },
    Saving,
}

impl Stage {
    pub fn label(self, language: Language) -> String {
        match self {
            Self::Checking => language.text("Checking the file", "Проверка файла").into(),
            Self::Searching => language
                .text("Searching MusicBrainz", "Поиск в MusicBrainz")
                .into(),
            Self::Loading { edition, total } => format!("Cover Art Archive: {edition}/{total}"),
            Self::Saving => language.text("Saving artwork", "Сохранение обложки").into(),
        }
    }
}

pub(crate) enum Outcome {
    Saved(Option<PathBuf>),
    HasCover,
    NotFound,
    Review(String),
    Failed(String),
    Cancelled,
}

impl Outcome {
    pub fn label(&self, language: Language) -> &'static str {
        match self {
            Self::Saved(_) => language.text("Saved", "Сохранено"),
            Self::HasCover => language.text("Already has artwork", "Обложка уже есть"),
            Self::NotFound => language.text("Not found", "Не найдено"),
            Self::Review(_) => language.text("Choose manually", "Выбрать вручную"),
            Self::Failed(_) => language.text("Error", "Ошибка"),
            Self::Cancelled => language.text("Cancelled", "Отменено"),
        }
    }

    pub fn details(&self, language: Language) -> String {
        match self {
            Self::Saved(Some(path)) => format!(
                "{}: {}",
                language.text("Original copy kept", "Исходная копия сохранена"),
                path.display()
            ),
            Self::Review(message) | Self::Failed(message) => message.clone(),
            _ => self.label(language).into(),
        }
    }
}

pub(crate) fn process(
    track: &Track,
    cancel: &AtomicBool,
    allowed: &Arc<Mutex<bool>>,
    progress: &mpsc::Sender<SearchProgress>,
    stage: &mpsc::Sender<Stage>,
    language: Language,
) -> Result<Outcome> {
    ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
    let _ = stage.send(Stage::Checking);
    let snapshot = FileSnapshot::read(&track.path)?;
    {
        let document = Document::read(&track.path)?;
        if document.blocks.iter().any(|block| !block.covers.is_empty()) {
            return Ok(Outcome::HasCover);
        }
        ensure!(
            document.blocks.iter().any(|block| block.writable),
            "Формат не поддерживает запись обложки."
        );
    }
    let _ = stage.send(Stage::Searching);
    let result = manual_search(track, cancel, progress)?;
    ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
    if result.truncated || result.warning.is_some() {
        return Ok(Outcome::Review(result.warning.unwrap_or_else(|| {
            language
                .text(
                    "The search is incomplete. Choose an edition manually.",
                    "Поиск неполный. Выберите издание вручную.",
                )
                .into()
        })));
    }
    if result.candidates.is_empty() {
        return Ok(Outcome::NotFound);
    }
    let candidates: Vec<_> = result
        .candidates
        .iter()
        .filter(|candidate| {
            !usable(&track.album) || normalized(&candidate.album) == normalized(&track.album)
        })
        .collect();
    if candidates.is_empty() {
        return Ok(Outcome::Review(
            language
                .text(
                    "The album in the file does not match the found editions.",
                    "Альбом в файле не совпал с найденными изданиями.",
                )
                .into(),
        ));
    }
    let albums: BTreeSet<_> = candidates
        .iter()
        .map(|candidate| normalized(&candidate.album))
        .collect();
    if albums.len() != 1 {
        return Ok(Outcome::Review(
            language
                .text(
                    "Several albums match. Choose the artwork manually.",
                    "Подходят несколько альбомов. Выберите обложку вручную.",
                )
                .into(),
        ));
    }
    let total = candidates.len();
    for (index, candidate) in candidates.into_iter().enumerate() {
        ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
        let _ = stage.send(Stage::Loading {
            edition: index + 1,
            total,
        });
        match preview(candidate) {
            Ok((bytes, _)) => {
                ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
                let _ = stage.send(Stage::Saving);
                return save_manual(track, &bytes, allowed, &snapshot).map(Outcome::Saved);
            }
            Err(error) if error.is::<NoCover>() => {}
            Err(error) => {
                return Err(error.context("Не удалось получить обложку из Cover Art Archive"));
            }
        }
    }
    Ok(Outcome::NotFound)
}
