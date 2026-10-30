use crate::{
    model::Track,
    tags::{Cover, Document},
};
use anyhow::{Context, Result, ensure};
use reqwest::{blocking::Client, redirect::Policy};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::{Cursor, Read},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const MAX_COVER: u64 = 8 * 1024 * 1024;

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

#[derive(Clone)]
pub(crate) struct FileSnapshot {
    length: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileSnapshot {
    pub(crate) fn read(path: &std::path::Path) -> Result<Self> {
        let metadata = std::fs::metadata(path).context("Аудиофайл недоступен")?;
        ensure!(metadata.is_file(), "Выберите локальный аудиофайл.");
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }

    fn matches(&self, other: &Self) -> bool {
        let same = self.length == other.length && self.modified == other.modified;
        #[cfg(unix)]
        let same = same && self.device == other.device && self.inode == other.inode;
        same
    }
}

pub(crate) fn manual_search(
    track: &Track,
    cancel: &AtomicBool,
    progress: &mpsc::Sender<SearchProgress>,
) -> Result<(Vec<Candidate>, bool)> {
    let query = crate::lyrics::Query {
        track: track.clone(),
        artist: if usable(&track.artist) {
            track.artist.clone()
        } else {
            String::new()
        },
        title: if usable(&track.title) {
            track.title.clone()
        } else {
            String::new()
        },
    };
    let guesses: Vec<_> = crate::lyrics::candidates(&query)
        .into_iter()
        .filter(|(artist, title)| usable(artist) && usable(title))
        .collect();
    ensure!(
        !guesses.is_empty(),
        "Не удалось определить исполнителя и название. Укажите их в тегах или имени файла."
    );
    ensure!(
        track.path.is_absolute() && track.path.is_file(),
        "Для поиска выберите локальный аудиофайл."
    );
    let client = client()?;
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    let mut truncated = false;
    'guesses: for (guess_artist, guess_title) in guesses {
        let query = format!(
            "recording:{} AND artist:{}",
            quoted(&guess_title),
            quoted(&guess_artist)
        );
        let mut offset = 0_u64;
        loop {
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
                if normalized(&artist) != normalized(&guess_artist)
                    || normalized(title) != normalized(&guess_title)
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
                        break 'guesses;
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
    snapshot: &FileSnapshot,
) -> Result<Option<std::path::PathBuf>> {
    use lofty::tag::ItemKey;
    ensure!(
        snapshot.matches(&FileSnapshot::read(&track.path)?),
        "Файл изменился после поиска. Повторите поиск обложки."
    );
    let mut document = Document::read(&track.path)?;
    ensure!(
        snapshot.matches(&FileSnapshot::read(&track.path)?),
        "Файл изменился после поиска. Повторите поиск обложки."
    );
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
            !usable(expected) || value.is_none_or(|value| !usable(value) || normalized(value) == normalized(expected)),
            "Теги файла изменились. Обновите библиотеку и повторите поиск."
        );
    }
    if usable(&track.album) {
        let album = primary
            .fields
            .iter()
            .find(|field| field.item.key() == ItemKey::AlbumTitle)
            .and_then(|field| field.item.value().text());
        ensure!(
            album.is_none_or(|value| !usable(value) || normalized(value) == normalized(&track.album)),
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
