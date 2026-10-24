use crate::{model::Track, network};
mod providers;
use anyhow::{Context, Result, ensure};
pub use providers::{Provider, ProviderReport, search, set_key};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lyric {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub provider: String,
    pub source: String,
    pub plain: String,
    pub synced: String,
    pub instrumental: bool,
    pub duration: Option<f64>,
    pub score: u16,
}

impl Lyric {
    pub fn content(&self) -> &str {
        if self.synced.is_empty() {
            &self.plain
        } else {
            &self.synced
        }
    }
    pub fn lines(&self) -> Vec<(Option<u64>, String)> {
        self.content()
            .lines()
            .flat_map(|line| {
                let mut rest = line;
                let mut times = Vec::new();
                while let Some(tail) = rest.strip_prefix('[') {
                    let Some(end) = tail.find(']') else {
                        break;
                    };
                    let Some((minutes, seconds)) = tail[..end].split_once(':') else {
                        break;
                    };
                    let Some(ms) = minutes.parse::<u64>().ok().and_then(|m| {
                        seconds
                            .parse::<f64>()
                            .ok()
                            .filter(|s| s.is_finite() && *s >= 0.0 && *s < 60.0)
                            .map(|s| m.saturating_mul(60_000).saturating_add((s * 1000.0) as u64))
                    }) else {
                        break;
                    };
                    times.push(ms);
                    rest = &tail[end + 1..];
                }
                if times.is_empty() {
                    vec![(None, line.to_owned())]
                } else {
                    times
                        .into_iter()
                        .map(|time| (Some(time), rest.trim().to_owned()))
                        .collect()
                }
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct Query {
    pub track: Track,
    pub artist: String,
    pub title: String,
}

pub struct SearchResult {
    pub matches: Vec<Lyric>,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcRecord {
    id: u64,
    #[serde(default)]
    track_name: String,
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    album_name: String,
    duration: Option<f64>,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if c == 'ё' {
                'е'
            } else if c.is_alphanumeric() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn cleaned(value: &str) -> String {
    let mut text = value.replace('_', " ");
    for (left, right) in [('(', ')'), ('[', ']')] {
        let mut position = 0;
        while let Some(start) = text[position..].find(left).map(|i| i + position) {
            let Some(end) = text[start..].find(right).map(|i| i + start) else {
                break;
            };
            let label = normalized(&text[start + left.len_utf8()..end]);
            if [
                "official video",
                "official audio",
                "official music video",
                "lyrics",
                "lyric video",
                "hd",
                "hq",
                "4k",
                "audio",
                "текст песни",
                "официальный клип",
            ]
            .contains(&label.as_str())
            {
                text.replace_range(start..end + right.len_utf8(), "");
                position = start;
            } else {
                position = end + right.len_utf8();
            }
        }
    }
    text.trim().into()
}

pub fn candidates(query: &Query) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let artist = cleaned(&query.artist);
    let title = cleaned(&query.title);
    if !title.is_empty() {
        result.push((artist.clone(), title.clone()));
    }
    let filename = query
        .track
        .path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let prefix = filename.bytes().take_while(u8::is_ascii_digit).count();
    let numbered = prefix > 0
        && prefix <= 3
        && (filename.starts_with('0') || filename[prefix..].starts_with(". "));
    let stripped = if numbered {
        filename[prefix..].trim_start_matches([' ', '.', '-', '_'])
    } else {
        &filename
    };
    for value in [
        query.title.as_str(),
        query.track.title.as_str(),
        filename.as_ref(),
        stripped,
    ] {
        let value = cleaned(value);
        for separator in [" - ", " — ", " – ", " | "] {
            if let Some((a, b)) = value.split_once(separator) {
                let (a, b) = (cleaned(a), cleaned(b));
                if !a.is_empty() && !b.is_empty() {
                    result.push((a.clone(), b.clone()));
                    result.push((b, a));
                }
                break;
            }
        }
        if !value.is_empty() {
            result.push((artist.clone(), value));
        }
    }
    let mut seen = HashSet::new();
    result.retain(|(artist, title)| seen.insert((normalized(artist), normalized(title))));
    result.truncate(6);
    result
}

fn similarity(left: &str, right: &str) -> f64 {
    let (left, right) = (normalized(left), normalized(right));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }
    if left.replace(' ', "") == right.replace(' ', "") {
        return 1.0;
    }
    let left = left.split_whitespace().collect::<HashSet<_>>();
    let right = right.split_whitespace().collect::<HashSet<_>>();
    2.0 * left.intersection(&right).count() as f64 / (left.len() + right.len()) as f64
}

fn rank(lyric: &Lyric, query: &Query, candidates: &[(String, String)]) -> u16 {
    let names = candidates
        .iter()
        .map(|(artist, title)| {
            let title = similarity(title, &lyric.title);
            if artist.is_empty() {
                title * 0.75
            } else {
                title * 0.65 + similarity(artist, &lyric.artist) * 0.35
            }
        })
        .fold(0.0, f64::max);
    let duration = lyric.duration.map_or(0.0, |seconds| {
        if query.track.duration_ms == 0 {
            0.0
        } else {
            let delta = (seconds - query.track.duration_ms as f64 / 1000.0).abs();
            if delta <= 3.0 {
                0.08
            } else if delta > 15.0 {
                -0.25
            } else {
                -0.05
            }
        }
    });
    ((names + duration).clamp(0.0, 1.0) * 1000.0) as u16
}

fn search_lrc(
    client: &reqwest::blocking::Client,
    artist: &str,
    title: &str,
) -> Result<Vec<LrcRecord>> {
    let mut request = client
        .get("https://lrclib.net/api/search")
        .query(&[("track_name", title)]);
    if !artist.is_empty() {
        request = request.query(&[("artist_name", artist)]);
    }
    request
        .send()
        .map_err(|_| anyhow::anyhow!("Не удалось связаться с LRCLIB."))
        .and_then(network::json)
}

fn add_lrc_records(
    records: Vec<LrcRecord>,
    query: &Query,
    guesses: &[(String, String)],
    ids: &mut HashSet<u64>,
    matches: &mut Vec<Lyric>,
    minimum_score: u16,
) {
    for record in records.into_iter().take(40) {
        if !ids.insert(record.id) {
            continue;
        }
        let mut lyric = Lyric {
            title: record.track_name,
            artist: record.artist_name,
            album: record.album_name,
            provider: "LRCLIB".into(),
            source: format!("https://lrclib.net/api/get/{}", record.id),
            plain: record.plain_lyrics.unwrap_or_default(),
            synced: record.synced_lyrics.unwrap_or_default(),
            instrumental: record.instrumental,
            duration: record.duration,
            score: 0,
        };
        if lyric.content().is_empty() && !lyric.instrumental {
            continue;
        }
        lyric.score = rank(&lyric, query, guesses);
        if lyric.score >= minimum_score {
            matches.push(lyric);
        }
    }
}

fn fallback_titles(guesses: &[(String, String)]) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for (_, title) in guesses {
        let title = normalized(title);
        if !title.chars().any(char::is_alphabetic) {
            continue;
        }
        let keywords = title
            .split_whitespace()
            .filter(|word| word.chars().count() >= 3)
            .collect::<Vec<_>>()
            .join(" ");
        for candidate in [title, keywords] {
            if !candidate.is_empty() && seen.insert(candidate.clone()) {
                result.push(candidate);
            }
        }
        if result.len() >= 4 {
            break;
        }
    }
    result.truncate(4);
    result
}

fn search_lrclib(query: &Query, cancel: &AtomicBool) -> Result<SearchResult> {
    let client = network::client()?;
    let guesses = candidates(query);
    ensure!(
        !guesses.is_empty(),
        "Укажите название композиции или исполнителя."
    );
    let mut result = SearchResult {
        matches: Vec::new(),
        warnings: Vec::new(),
    };
    let mut ids = HashSet::new();
    let mut lrc_available = true;
    for (artist, title) in &guesses {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match search_lrc(&client, artist, title) {
            Ok(records) => {
                add_lrc_records(records, query, &guesses, &mut ids, &mut result.matches, 0);
            }
            Err(error) => {
                result.warnings.push(error.to_string());
                lrc_available = false;
                break;
            }
        }
        if result.matches.iter().any(|m| m.score >= 980) {
            break;
        }
    }
    // File names often omit punctuation in artist names or add short words to
    // titles. Broaden retrieval, but still rank against the original metadata.
    if lrc_available && !result.matches.iter().any(|lyric| lyric.score >= 900) {
        for title in fallback_titles(&guesses) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match search_lrc(&client, "", &title) {
                Ok(records) => {
                    add_lrc_records(records, query, &guesses, &mut ids, &mut result.matches, 600);
                }
                Err(error) => {
                    result.warnings.push(error.to_string());
                    break;
                }
            }
            if result.matches.iter().any(|lyric| lyric.score >= 900) {
                break;
            }
        }
    }
    result
        .matches
        .sort_by_key(|lyric| std::cmp::Reverse(lyric.score));
    result.matches.truncate(40);
    Ok(result)
}

fn search_ovh(query: &Query, cancel: &AtomicBool) -> Result<SearchResult> {
    let client = network::client()?;
    let guesses = candidates(query);
    let mut result = SearchResult {
        matches: vec![],
        warnings: vec![],
    };
    {
        for (artist, title) in guesses.iter().filter(|(a, _)| !a.is_empty()).take(3) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let mut url = network::public_url("https://api.lyrics.ovh/v1/")?;
            url.path_segments_mut()
                .expect("HTTP base")
                .pop_if_empty()
                .extend([artist, title]);
            let response = client.get(url).send();
            let value: Result<serde_json::Value> = match response {
                Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => continue,
                Ok(response) => network::json(response),
                Err(_) => Err(anyhow::anyhow!("Не удалось связаться с lyrics.ovh.")),
            };
            match value {
                Ok(value) => {
                    if let Some(plain) = value
                        .get("lyrics")
                        .and_then(|v| v.as_str())
                        .filter(|v| !v.trim().is_empty())
                    {
                        let mut lyric = Lyric {
                            title: title.clone(),
                            artist: artist.clone(),
                            album: String::new(),
                            provider: "lyrics.ovh".into(),
                            source: "https://lyrics.ovh/".into(),
                            plain: plain.into(),
                            synced: String::new(),
                            instrumental: false,
                            duration: None,
                            score: 0,
                        };
                        lyric.score = rank(&lyric, query, &guesses);
                        result.matches.push(lyric);
                        break;
                    }
                }
                Err(error) => {
                    result.warnings.push(error.to_string());
                    break;
                }
            }
        }
    }
    result
        .matches
        .sort_by_key(|lyric| std::cmp::Reverse(lyric.score));
    result.matches.truncate(40);
    Ok(result)
}

pub fn embedded(track: &Track) -> Option<Lyric> {
    let document = crate::tags::Document::read(&track.path).ok()?;
    let text = document
        .blocks
        .iter()
        .flat_map(|block| &block.fields)
        .find_map(|field| {
            (field.item.key() == lofty::tag::ItemKey::Lyrics)
                .then(|| field.item.value().text())
                .flatten()
        })?;
    if text.is_empty() {
        return None;
    }
    Some(Lyric {
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        provider: "File".into(),
        source: track.path.display().to_string(),
        plain: text.into(),
        synced: String::new(),
        instrumental: false,
        duration: Some(track.duration_ms as f64 / 1000.0),
        score: 1000,
    })
}

pub fn cache_path(data: &Path, track: i64) -> PathBuf {
    data.join("lyrics").join(format!("{track}.json"))
}
pub fn save(data: &Path, track: i64, lyric: &Lyric) -> Result<()> {
    network::save_json(&cache_path(data, track), lyric)
}
pub fn cached(data: &Path, track: i64) -> Result<Option<Lyric>> {
    let path = cache_path(data, track);
    match std::fs::metadata(&path) {
        Ok(_) => network::load_json(&path).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub fn remove(data: &Path, track: i64) -> Result<()> {
    let path = cache_path(data, track);
    if path.try_exists()? {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
pub fn export(path: &Path, lyric: &Lyric) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("Выберите папку сохранения.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(lyric.content().as_bytes())?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|e| e.error)
        .context("Файл уже существует или папка недоступна.")?;
    Ok(())
}
