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
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_COVER: u64 = 8 * 1024 * 1024;
const NEGATIVE_TTL: u64 = 30 * 24 * 60 * 60;

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
) -> Result<Option<(String, String)>> {
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
            return Ok(None);
        }
        if offset > 0 {
            std::thread::sleep(Duration::from_millis(1200));
        }
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(None);
        }
        let response = client
            .get("https://musicbrainz.org/ws/2/recording/")
            .query(&[
                ("query", query.as_str()),
                ("fmt", "json"),
                ("limit", "100"),
                ("offset", &offset.to_string()),
            ])
            .send()?;
        let body: Value = crate::network::json(response)?;
        let count = body["count"]
            .as_u64()
            .context("MusicBrainz не вернул число записей.")?;
        ensure!(
            count == 0 || body["recordings"].is_array(),
            "MusicBrainz не вернул список записей."
        );
        if count > 500 {
            return Ok(None);
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
        Ok(groups
            .into_iter()
            .next()
            .map(|id| ("release-group".into(), id)))
    } else if groups.is_empty() && releases.len() == 1 {
        Ok(releases.into_iter().next().map(|id| ("release".into(), id)))
    } else {
        Ok(None)
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

/// Returns true only after an embedded cover has been safely written.
pub(crate) fn find_and_save(
    track: &Track,
    cache: &Path,
    enabled: &Arc<Mutex<bool>>,
) -> Result<bool> {
    if !usable(&track.artist)
        || !usable(&track.title)
        || !usable(&track.album)
        || !track.path.is_absolute()
        || !track.path.is_file()
    {
        return Ok(false);
    }
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(false);
    }
    let mut document = Document::read(&track.path)?;
    if !metadata_matches(&document, track)
        || document.blocks.iter().any(|block| !block.covers.is_empty())
        || !document.blocks.iter().any(|block| block.writable)
    {
        return Ok(false);
    }
    let cached = cache_path(track, cache);
    if crate::network::load_json::<u64>(&cached)
        .is_ok_and(|time| now().saturating_sub(time) < NEGATIVE_TTL)
    {
        return Ok(false);
    }
    let client = client()?;
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(false);
    }
    let Some((kind, id)) = matching_release(&client, track, enabled)? else {
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(false);
        }
        let _ = crate::network::save_json(&cached, &now());
        return Ok(false);
    };
    if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
        return Ok(false);
    }
    let response = client
        .get(format!("https://coverartarchive.org/{kind}/{id}/front-500"))
        .send()?;
    let response = if response.status() == reqwest::StatusCode::NOT_FOUND {
        if !*enabled.lock().unwrap_or_else(|error| error.into_inner()) {
            return Ok(false);
        }
        client
            .get(format!("https://coverartarchive.org/{kind}/{id}/front"))
            .send()?
    } else {
        response
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        let _ = crate::network::save_json(&cached, &now());
        return Ok(false);
    }
    let response = response.error_for_status()?;
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_COVER),
        "Обложка слишком большая."
    );
    let mut bytes = Vec::new();
    response.take(MAX_COVER + 1).read_to_end(&mut bytes)?;
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
        return Ok(false);
    }
    let prepared = document.prepare()?;
    let guard = enabled.lock().unwrap_or_else(|error| error.into_inner());
    if !*guard {
        return Ok(false);
    }
    prepared.replace()?.accept()?;
    Ok(true)
}
