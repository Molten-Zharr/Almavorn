use crate::{
    model::Track,
    tags::{Cover, Document},
};
use anyhow::{Context, Result, ensure};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_COVER: u64 = 8 * 1024 * 1024;
const NEGATIVE_TTL: u64 = 30 * 24 * 60 * 60;

static LAST_MUSICBRAINZ_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

fn wait_for_musicbrainz() {
    let mut previous = LAST_MUSICBRAINZ_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(last) = *previous {
        let delay = Duration::from_millis(1200).saturating_sub(last.elapsed());
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    *previous = Some(Instant::now());
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub id: String,
    pub album: String,
    pub artist: String,
    pub date: String,
}

#[derive(Clone, Copy)]
pub(crate) struct SearchProgress {
    pub checked: u64,
    pub total: u64,
    pub editions: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AutoOutcome {
    Saved,
    HasCover,
    NotLocal,
    MissingMetadata,
    MismatchedMetadata,
    Unsupported,
    NoMatch,
    Ambiguous,
    TooMany,
    NoCover,
    CachedNoMatch,
    Cancelled,
}

enum MatchOutcome {
    Matched(String, String),
    NoMatch,
    Ambiguous,
    TooMany,
}

pub(crate) fn manual_search(
    track: &Track,
    cancel: &AtomicBool,
    progress: &mpsc::Sender<SearchProgress>,
) -> Result<(Vec<Candidate>, bool)> {
    ensure!(
        usable(&track.artist) && usable(&track.title),
        "У композиции нужны исполнитель и название для поиска."
    );
    ensure!(
        track.path.is_absolute() && track.path.is_file(),
        "Для поиска выберите локальный аудиофайл."
    );
    let query = format!(
        "recording:{} AND artist:{}",
        quoted(&track.title),
        quoted(&track.artist)
    );
    let client = client()?;
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    let mut offset = 0_u64;
    let mut truncated = false;
    'pages: loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
        wait_for_musicbrainz();
        ensure!(!cancel.load(Ordering::Relaxed), "Поиск отменен.");
        let response = client
            .get("https://musicbrainz.org/ws/2/recording/")
            .query(&[
                ("query", query.as_str()),
                ("fmt", "json"),
                ("limit", "100"),
                ("offset", &offset.to_string()),
            ])
            .send()
            .context("MusicBrainz недоступен")?;
        let body: Value = crate::network::json(response)?;
        let count = body["count"]
            .as_u64()
            .context("MusicBrainz не вернул число записей.")?;
        ensure!(
            count == 0 || body["recordings"].is_array(),
            "MusicBrainz не вернул список записей."
        );
        let recordings = body["recordings"].as_array();
        let received = recordings.map_or(0, Vec::len) as u64;
        ensure!(
            received > 0 || offset >= count,
            "MusicBrainz вернул неполный список записей."
        );
        for recording in recordings.into_iter().flatten() {
            let artist = credit(recording);
            let title = recording["title"].as_str().unwrap_or_default();
            if normalized(&artist) != normalized(&track.artist)
                || normalized(title) != normalized(&track.title)
            {
                continue;
            }
            for release in recording["releases"].as_array().into_iter().flatten() {
                let Some(id) = release["id"].as_str().filter(|id| valid_id(id)) else {
                    continue;
                };
                if !seen.insert(id.to_owned()) {
                    continue;
                }
                let album = release["title"].as_str().unwrap_or_default().trim();
                if album.is_empty() {
                    continue;
                }
                results.push(Candidate {
                    id: id.into(),
                    album: album.into(),
                    artist: artist.clone(),
                    date: release["date"].as_str().unwrap_or_default().into(),
                });
                if results.len() >= 1000 {
                    truncated = true;
                    break 'pages;
                }
            }
        }
        offset += received;
        let _ = progress.send(SearchProgress {
            checked: offset,
            total: count,
            editions: results.len(),
        });
        if offset >= count {
            break;
        }
        if offset >= 500 {
            truncated = true;
            break;
        }
    }
    results.sort_by_key(|candidate| normalized(&candidate.album) != normalized(&track.album));
    if results.len() > 100 {
        results.truncate(100);
        truncated = true;
    }
    Ok((results, truncated))
}

fn normalized(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn usable(value: &str) -> bool {
    let value = normalized(value);
    !value.is_empty()
        && !matches!(
            value.as_str(),
            "unknown"
                | "unknown artist"
                | "unknown album"
                | "неизвестный артист"
                | "неизвестный альбом"
        )
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn valid_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(concat!(
            "Almavorn/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/Molten-Zharr/Almavorn)"
        ))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .redirect(Policy::custom(|attempt| {
            let url = attempt.url();
            let allowed = url.scheme() == "https"
                && url.host_str().is_some_and(|host| {
                    host == "coverartarchive.org"
                        || host == "archive.org"
                        || host.ends_with(".archive.org")
                });
            if allowed && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()?)
}

fn credit(recording: &Value) -> String {
    recording["artist-credit"]
        .as_array()
        .map(|credits| {
            credits
                .iter()
                .map(|part| {
                    format!(
                        "{}{}",
                        part["name"]
                            .as_str()
                            .or_else(|| part["artist"]["name"].as_str())
                            .unwrap_or(""),
                        part["joinphrase"].as_str().unwrap_or("")
                    )
                })
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn matching_release(
    client: &Client,
    track: &Track,
    enabled: &Arc<Mutex<bool>>,
) -> Result<MatchOutcome> {
    let query = format!(
        "recording:{} AND artist:{}",
        quoted(&track.title),
        quoted(&track.artist)
    );
    let mut groups = BTreeSet::new();
    let mut releases = BTreeSet::new();
    let mut offset = 0_u64;
    loop {
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(MatchOutcome::NoMatch);
        }
        wait_for_musicbrainz();
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(MatchOutcome::NoMatch);
        }
        let response = client
            .get("https://musicbrainz.org/ws/2/recording/")
            .query(&[
                ("query", query.as_str()),
                ("fmt", "json"),
                ("limit", "100"),
                ("offset", &offset.to_string()),
            ])
            .send()
            .context("MusicBrainz недоступен")?;
        let body: Value = crate::network::json(response)?;
        let count = body["count"]
            .as_u64()
            .context("MusicBrainz не вернул число записей.")?;
        ensure!(
            count == 0 || body["recordings"].is_array(),
            "MusicBrainz не вернул список записей."
        );
        if count > 500 {
            return Ok(MatchOutcome::TooMany);
        }
        let recordings = body["recordings"].as_array();
        let received = recordings.map_or(0, Vec::len) as u64;
        ensure!(
            received > 0 || offset >= count,
            "MusicBrainz вернул неполный список записей."
        );
        for recording in recordings.into_iter().flatten() {
            if normalized(recording["title"].as_str().unwrap_or("")) != normalized(&track.title)
                || normalized(&credit(recording)) != normalized(&track.artist)
            {
                continue;
            }
            if let Some(length) = recording["length"].as_u64()
                && track.duration_ms > 0
                && length.abs_diff(track.duration_ms) > 3000
            {
                continue;
            }
            for release in recording["releases"].as_array().into_iter().flatten() {
                if normalized(release["title"].as_str().unwrap_or("")) != normalized(&track.album) {
                    continue;
                }
                if let Some(id) = release["release-group"]["id"]
                    .as_str()
                    .filter(|id| valid_id(id))
                {
                    groups.insert(id.to_owned());
                } else if let Some(id) = release["id"].as_str().filter(|id| valid_id(id)) {
                    releases.insert(id.to_owned());
                }
            }
        }
        offset += received;
        if offset >= count {
            break;
        }
    }
    if groups.len() == 1 && releases.is_empty() {
        Ok(MatchOutcome::Matched(
            "release-group".into(),
            groups.into_iter().next().expect("one group"),
        ))
    } else if groups.is_empty() && releases.len() == 1 {
        Ok(MatchOutcome::Matched(
            "release".into(),
            releases.into_iter().next().expect("one release"),
        ))
    } else if !groups.is_empty() || !releases.is_empty() {
        Ok(MatchOutcome::Ambiguous)
    } else {
        Ok(MatchOutcome::NoMatch)
    }
}

fn cache_path(track: &Track, cache: &Path) -> std::path::PathBuf {
    let key = format!(
        "{}\0{}\0{}\0{}",
        normalized(&track.artist),
        normalized(&track.title),
        normalized(&track.album),
        track.duration_ms
    );
    cache.join(format!("{:x}.json", Sha256::digest(key.as_bytes())))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn metadata_matches(document: &Document, track: &Track) -> bool {
    use lofty::tag::ItemKey;
    let Some(primary) = document.blocks.first() else {
        return false;
    };
    [
        (ItemKey::TrackTitle, &track.title),
        (ItemKey::TrackArtist, &track.artist),
        (ItemKey::AlbumTitle, &track.album),
    ]
    .into_iter()
    .all(|(key, expected)| {
        primary
            .fields
            .iter()
            .find(|field| field.item.key() == key)
            .and_then(|field| field.item.value().text())
            .is_some_and(|value| normalized(value) == normalized(expected))
    })
}

/// Read the file first and write only an unambiguous missing cover.
pub(crate) fn find_and_save(
    track: &Track,
    cache: &Path,
    enabled: &Arc<Mutex<bool>>,
) -> Result<AutoOutcome> {
    if !track.path.is_absolute() || !track.path.is_file() {
        return Ok(AutoOutcome::NotLocal);
    }
    if !usable(&track.artist) || !usable(&track.title) || !usable(&track.album) {
        return Ok(AutoOutcome::MissingMetadata);
    }
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(AutoOutcome::Cancelled);
    }
    let mut document = Document::read(&track.path)?;
    if !metadata_matches(&document, track) {
        return Ok(AutoOutcome::MismatchedMetadata);
    }
    if document.blocks.iter().any(|block| !block.covers.is_empty()) {
        return Ok(AutoOutcome::HasCover);
    }
    if !document.blocks.iter().any(|block| block.writable) {
        return Ok(AutoOutcome::Unsupported);
    }
    let cached = cache_path(track, cache);
    if crate::network::load_json::<u64>(&cached)
        .is_ok_and(|time| now().saturating_sub(time) < NEGATIVE_TTL)
    {
        return Ok(AutoOutcome::CachedNoMatch);
    }
    let client = client()?;
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(AutoOutcome::Cancelled);
    }
    let matched = matching_release(&client, track, enabled)?;
    let (kind, id) = match matched {
        MatchOutcome::Matched(kind, id) => (kind, id),
        other => {
            if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
                return Ok(AutoOutcome::Cancelled);
            }
            let _ = crate::network::save_json(&cached, &now());
            return Ok(match other {
                MatchOutcome::NoMatch => AutoOutcome::NoMatch,
                MatchOutcome::Ambiguous => AutoOutcome::Ambiguous,
                MatchOutcome::TooMany => AutoOutcome::TooMany,
                MatchOutcome::Matched(_, _) => unreachable!(),
            });
        }
    };
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(AutoOutcome::Cancelled);
    }
    let response = client
        .get(format!("https://coverartarchive.org/{kind}/{id}/front-500"))
        .send()
        .context("Cover Art Archive недоступен")?;
    let response = if response.status() == reqwest::StatusCode::NOT_FOUND {
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(AutoOutcome::Cancelled);
        }
        client
            .get(format!("https://coverartarchive.org/{kind}/{id}/front"))
            .send()
            .context("Cover Art Archive недоступен")?
    } else {
        response
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        let _ = crate::network::save_json(&cached, &now());
        return Ok(AutoOutcome::NoCover);
    }
    let response = response
        .error_for_status()
        .context("Ошибка Cover Art Archive")?;
    ensure!(
        response.status().is_success(),
        "Cover Art Archive перенаправил запрос на недоступный адрес."
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_COVER),
        "Обложка слишком большая."
    );
    let mut bytes = Vec::new();
    response
        .take(MAX_COVER + 1)
        .read_to_end(&mut bytes)
        .context("Не удалось загрузить обложку из Cover Art Archive")?;
    ensure!(bytes.len() as u64 <= MAX_COVER, "Обложка слишком большая.");
    crate::artwork::validate_bytes(&bytes)?;
    let mut picture = lofty::picture::Picture::from_reader(&mut Cursor::new(bytes))?;
    picture.set_pic_type(lofty::picture::PictureType::CoverFront);
    document
        .blocks
        .iter_mut()
        .find(|block| block.writable)
        .context("Формат не поддерживает запись обложки.")?
        .covers
        .push(Cover {
            picture,
            remove: false,
        });
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(AutoOutcome::Cancelled);
    }
    let prepared = document.prepare()?;
    let guard = enabled.lock().unwrap_or_else(|error| error.into_inner());
    if !*guard {
        return Ok(AutoOutcome::Cancelled);
    }
    prepared.replace()?.accept()?;
    Ok(AutoOutcome::Saved)
}

pub(crate) fn preview(candidate: &Candidate) -> Result<(Vec<u8>, crate::artwork::ArtworkImage)> {
    ensure!(
        valid_id(&candidate.id),
        "Некорректный идентификатор обложки."
    );
    let client = client()?;
    let response = client
        .get(format!(
            "https://coverartarchive.org/release/{}/front-500",
            candidate.id
        ))
        .send()?;
    let response = if response.status() == reqwest::StatusCode::NOT_FOUND {
        client
            .get(format!(
                "https://coverartarchive.org/release/{}/front",
                candidate.id
            ))
            .send()?
    } else {
        response
    };
    ensure!(
        response.status() != reqwest::StatusCode::NOT_FOUND,
        "Для этого издания обложка не найдена."
    );
    let response = response.error_for_status()?;
    ensure!(
        response.status().is_success(),
        "Cover Art Archive перенаправил запрос на недоступный адрес."
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_COVER),
        "Обложка больше 8 МБ."
    );
    let mut bytes = Vec::new();
    response.take(MAX_COVER + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_COVER, "Обложка больше 8 МБ.");
    let image = crate::artwork::decode(&bytes)?;
    Ok((bytes, image))
}

pub(crate) fn save_manual(
    track: &Track,
    bytes: &[u8],
    allowed: &Arc<Mutex<bool>>,
) -> Result<Option<std::path::PathBuf>> {
    use lofty::tag::ItemKey;
    let mut document = Document::read(&track.path)?;
    let primary = document
        .blocks
        .first()
        .context("Теги композиции недоступны.")?;
    for (key, expected) in [
        (ItemKey::TrackArtist, track.artist.as_str()),
        (ItemKey::TrackTitle, track.title.as_str()),
    ] {
        let value = primary
            .fields
            .iter()
            .find(|field| field.item.key() == key)
            .and_then(|field| field.item.value().text());
        ensure!(
            value.is_some_and(|value| normalized(value) == normalized(expected)),
            "Теги файла изменились или не содержат исполнителя и название. Обновите библиотеку и повторите поиск."
        );
    }
    if usable(&track.album) {
        let album = primary
            .fields
            .iter()
            .find(|field| field.item.key() == ItemKey::AlbumTitle)
            .and_then(|field| field.item.value().text());
        ensure!(
            album.is_some_and(|value| normalized(value) == normalized(&track.album)),
            "Альбом в файле изменился. Обновите библиотеку и повторите поиск."
        );
    }
    ensure!(
        document.blocks.iter().all(|block| block.writable
            || !block
                .covers
                .iter()
                .any(|cover| cover.picture.pic_type() == lofty::picture::PictureType::CoverFront)),
        "Существующую обложку в этом формате безопасно заменить не удалось."
    );
    for block in &mut document.blocks {
        if block.writable {
            block.covers.retain(|cover| {
                cover.picture.pic_type() != lofty::picture::PictureType::CoverFront
            });
        }
    }
    crate::artwork::validate_bytes(bytes)?;
    let mut picture = lofty::picture::Picture::from_reader(&mut Cursor::new(bytes))?;
    picture.set_pic_type(lofty::picture::PictureType::CoverFront);
    document
        .blocks
        .iter_mut()
        .find(|block| block.writable)
        .context("Формат не поддерживает запись обложки.")?
        .covers
        .push(Cover {
            picture,
            remove: false,
        });
    if !*allowed.lock().unwrap_or_else(|error| error.into_inner()) {
        anyhow::bail!("Сохранение отменено.");
    }
    let prepared = document.prepare()?;
    let guard = allowed.lock().unwrap_or_else(|error| error.into_inner());
    ensure!(*guard, "Сохранение отменено.");
    prepared.replace()?.accept()
}
