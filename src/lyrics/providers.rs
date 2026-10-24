use super::{Lyric, Query, SearchResult, candidates, normalized, rank};
use crate::{network, secrets, services};
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Url, blocking::Client};
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use std::{
    collections::HashSet,
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Genius,
    Lrclib,
    Musixmatch,
    Yandex,
    Ovh,
    Azlyrics,
    Songlyrics,
    Elyrics,
    Letras,
}

impl Provider {
    pub const ALL: [Self; 9] = [
        Self::Genius,
        Self::Lrclib,
        Self::Musixmatch,
        Self::Yandex,
        Self::Ovh,
        Self::Azlyrics,
        Self::Songlyrics,
        Self::Elyrics,
        Self::Letras,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Genius => "Genius",
            Self::Lrclib => "LRCLIB",
            Self::Musixmatch => "Musixmatch",
            Self::Yandex => "Яндекс Музыка",
            Self::Ovh => "lyrics.ovh",
            Self::Azlyrics => "AZLyrics",
            Self::Songlyrics => "SongLyrics",
            Self::Elyrics => "eLyrics",
            Self::Letras => "Letras",
        }
    }
    fn host(self) -> &'static str {
        match self {
            Self::Genius => "genius.com",
            Self::Musixmatch => "musixmatch.com",
            Self::Azlyrics => "azlyrics.com",
            Self::Songlyrics => "songlyrics.com",
            Self::Elyrics => "elyrics.net",
            Self::Letras => "letras.mus.br",
            Self::Lrclib => "lrclib.net",
            Self::Ovh => "lyrics.ovh",
            Self::Yandex => "music.yandex.ru",
        }
    }
    fn key_id(self) -> Result<&'static str> {
        match self {
            Self::Genius => Ok("lyrics-genius"),
            Self::Musixmatch => Ok("lyrics-musixmatch"),
            _ => bail!("Для этого источника отдельный ключ не нужен."),
        }
    }
    pub fn key_url(self) -> &'static str {
        match self {
            Self::Genius => "https://genius.com/api-clients",
            Self::Musixmatch => "https://developer.musixmatch.com/",
            _ => "",
        }
    }
}

#[derive(Clone)]
pub struct ProviderReport {
    pub provider: Provider,
    pub count: usize,
    pub error: Option<String>,
}

// Secrets are only read or changed on blocking workers, never on the UI thread.
pub fn set_key(data: &Path, provider: Provider, key: Option<&str>) -> Result<()> {
    let id = provider.key_id()?;
    if let Some(key) = key {
        ensure!(
            !key.is_empty() && key.len() <= 4096 && key.bytes().all(|b| b.is_ascii_graphic()),
            "Введите ключ без пробелов и переносов строк."
        );
        secrets::set(data, id, key)
    } else {
        secrets::remove(data, id)
    }
}

pub fn search(
    data: &Path,
    query: &Query,
    cancel: &AtomicBool,
    progress: &Sender<ProviderReport>,
) -> Result<SearchResult> {
    ensure!(
        !candidates(query).is_empty(),
        "Укажите название композиции или исполнителя."
    );
    let mut result = SearchResult {
        matches: vec![],
        warnings: vec![],
    };
    // Sources run concurrently. Cancellation is checked between bounded requests;
    // the UI discards results from older search generations.
    std::thread::scope(|scope| {
        let jobs = Provider::ALL.map(|provider| {
            (
                provider,
                scope.spawn(move || {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(SearchResult {
                            matches: vec![],
                            warnings: vec![],
                        });
                    }
                    let found = search_provider(provider, data, query, cancel);
                    let report = match &found {
                        Ok(value) => ProviderReport {
                            provider,
                            count: value.matches.len(),
                            error: (!value.warnings.is_empty()).then(|| value.warnings.join(" ")),
                        },
                        Err(error) => ProviderReport {
                            provider,
                            count: 0,
                            error: Some(error.to_string()),
                        },
                    };
                    let _ = progress.send(report);
                    found
                }),
            )
        });
        for (provider, job) in jobs {
            match job.join() {
                Ok(Ok(found)) => {
                    result.matches.extend(found.matches);
                    result.warnings.extend(
                        found
                            .warnings
                            .into_iter()
                            .map(|w| format!("{}: {w}", provider.name())),
                    );
                }
                Ok(Err(error)) => result
                    .warnings
                    .push(format!("{}: {error}", provider.name())),
                Err(_) => {
                    let error = "Поиск прерван. Повторите попытку.".to_owned();
                    let _ = progress.send(ProviderReport {
                        provider,
                        count: 0,
                        error: Some(error.clone()),
                    });
                    result
                        .warnings
                        .push(format!("{}: {error}", provider.name()));
                }
            }
        }
    });
    let mut seen = HashSet::new();
    result.matches.retain(|lyric| {
        seen.insert((
            lyric.provider.clone(),
            lyric.source.clone(),
            lyric.content().to_owned(),
        ))
    });
    // Prefer Genius among convincing matches; uncertain guesses stay below them.
    result.matches.sort_by_key(|lyric| {
        std::cmp::Reverse((
            lyric.score >= 900,
            lyric.score >= 900 && lyric.provider == "Genius",
            lyric.score,
        ))
    });
    result.matches.truncate(40);
    Ok(result)
}

fn search_provider(
    provider: Provider,
    data: &Path,
    query: &Query,
    cancel: &AtomicBool,
) -> Result<SearchResult> {
    if provider == Provider::Lrclib {
        return super::search_lrclib(query, cancel);
    }
    if provider == Provider::Ovh {
        return super::search_ovh(query, cancel);
    }
    if provider == Provider::Yandex {
        return yandex(data, query, cancel);
    }
    let client = client(provider)?;
    let guesses = candidates(query);
    match provider {
        Provider::Genius => genius(&client, data, query, &guesses, cancel),
        Provider::Musixmatch => musixmatch(&client, data, query, &guesses, cancel),
        _ => pages(&client, provider, query, &guesses, cancel).map(|matches| SearchResult {
            matches,
            warnings: vec![],
        }),
    }
}

fn allowed(url: &Url, provider: Provider) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| {
            host == provider.host() || host.ends_with(&format!(".{}", provider.host()))
        })
}

fn client(provider: Provider) -> Result<Client> {
    Client::builder()
        .user_agent(concat!(
            "Almavorn/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/Molten-Zharr/Almavorn)"
        ))
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() < 4 && allowed(attempt.url(), provider) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .context("Не удалось подготовить подключение.")
}

fn get_page(client: &Client, provider: Provider, url: &str) -> Result<Option<String>> {
    let url = Url::parse(url).context("Источник вернул неправильную ссылку.")?;
    ensure!(
        allowed(&url, provider),
        "Источник вернул ссылку на другой сайт."
    );
    let response = client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "ru,en;q=0.8")
        .send()
        .map_err(|_| anyhow::anyhow!("Нет ответа от сервиса."))?;
    let status = response.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    ensure!(
        (200..300).contains(&status),
        "{} (HTTP {status}).",
        match status {
            401 => "Нужен вход",
            403 => "Сайт ограничил доступ",
            429 => "Слишком много запросов. Повторите позже",
            _ => "Сайт временно недоступен",
        }
    );
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .context("Не удалось прочитать страницу.")?;
    ensure!(bytes.len() <= 4 * 1024 * 1024, "Страница слишком большая.");
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("static lyrics selector")
}

fn text(element: ElementRef<'_>, output: &mut String, depth: usize) {
    if depth > 128 || output.len() > 256 * 1024 {
        return;
    }
    let tag = element.value().name();
    if matches!(
        tag,
        "script" | "style" | "button" | "svg" | "iframe" | "noscript"
    ) || element.attr("aria-hidden") == Some("true")
        || element.attr("data-exclude-from-selection").is_some()
    {
        return;
    }
    let block = matches!(tag, "p" | "div" | "br" | "section");
    if block && !output.ends_with('\n') {
        output.push('\n');
    }
    for child in element.children() {
        if let Some(element) = ElementRef::wrap(child) {
            text(element, output, depth + 1);
        } else if let scraper::Node::Text(value) = child.value() {
            output.push_str(value);
        }
    }
    if block && !output.ends_with('\n') {
        output.push('\n');
    }
}

fn lyrics_text(document: &Html, provider: Provider) -> Result<String> {
    let mut result = String::new();
    let css = match provider {
        Provider::Genius => "[data-lyrics-container=true], .lyrics",
        Provider::Songlyrics => "#songLyricsDiv",
        Provider::Elyrics => "#inlyr",
        Provider::Letras => ".lyric-original",
        Provider::Azlyrics => "div",
        _ => unreachable!(),
    };
    for element in document.select(&selector(css)) {
        if provider == Provider::Azlyrics && !element.children().any(|child| {
            matches!(child.value(), scraper::Node::Comment(comment) if comment.contains("azlyrics.com content"))
        }) { continue; }
        text(element, &mut result, 0);
        result.push('\n');
    }
    let result = result
        .lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    ensure!(
        !result.is_empty(),
        "На странице не удалось прочитать текст: сайт изменился или ограничил доступ."
    );
    ensure!(result.len() <= 256 * 1024, "Текст слишком большой.");
    Ok(result)
}

fn lyric(provider: Provider, artist: &str, title: &str, source: String, plain: String) -> Lyric {
    Lyric {
        title: title.into(),
        artist: artist.into(),
        album: String::new(),
        provider: provider.name().into(),
        source,
        plain,
        synced: String::new(),
        instrumental: false,
        duration: None,
        score: 0,
    }
}

fn request_json(request: reqwest::blocking::RequestBuilder) -> Result<Value> {
    network::json(
        request
            .send()
            .map_err(|_| anyhow::anyhow!("Нет ответа от сервиса."))?,
    )
}

fn genius(
    client: &Client,
    data: &Path,
    query: &Query,
    guesses: &[(String, String)],
    cancel: &AtomicBool,
) -> Result<SearchResult> {
    // The public website search works without an account. An optional personal
    // token uses the official API instead; it is never sent to page URLs.
    let key = secrets::get(data, Provider::Genius.key_id()?).unwrap_or(None);
    let mut found = vec![];
    let mut warnings = vec![];
    let mut seen = HashSet::new();
    let mut queries = HashSet::new();
    let attempts = guesses
        .iter()
        .take(3)
        .map(|(artist, title)| format!("{artist} {title}"))
        .chain(guesses.iter().take(2).map(|(_, title)| title.clone()));
    let operation = (|| -> Result<()> {
        for search in attempts {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            if !queries.insert(normalized(&search)) {
                continue;
            }
            let request = if let Some(key) = &key {
                client.get("https://api.genius.com/search").bearer_auth(key)
            } else {
                client.get("https://genius.com/api/search")
            };
            let value = request_json(request.query(&[("q", search.as_str())]))?;
            let hits = value["response"]["hits"]
                .as_array()
                .context("Genius вернул неизвестный ответ поиска.")?;
            let mut pending = vec![];
            for hit in hits.iter().take(20) {
                let hit = &hit["result"];
                let Some(url) = hit["url"].as_str() else {
                    continue;
                };
                let mut item = lyric(
                    Provider::Genius,
                    hit["primary_artist"]["name"].as_str().unwrap_or(""),
                    hit["title"].as_str().unwrap_or(""),
                    url.into(),
                    String::new(),
                );
                item.score = rank(&item, query, guesses);
                // Genius appends English translations to many Russian titles.
                // Keep the display title, but also compare its original part.
                if let Some((original, translation)) = item.title.rsplit_once(" (")
                    && original
                        .chars()
                        .any(|c| ('\u{0400}'..='\u{04ff}').contains(&c))
                    && translation.ends_with(')')
                    && translation.is_ascii()
                    && ![
                        "live",
                        "remix",
                        "remaster",
                        "version",
                        "demo",
                        "acoustic",
                        "instrumental",
                        "edit",
                    ]
                    .iter()
                    .any(|word| translation.to_lowercase().contains(word))
                {
                    let mut original_title = item.clone();
                    original_title.title = original.to_owned();
                    item.score = item.score.max(rank(&original_title, query, guesses));
                }
                if item.score >= 600 && seen.insert(url.to_owned()) {
                    pending.push(item);
                }
            }
            pending.sort_by_key(|item| std::cmp::Reverse(item.score));
            for mut item in pending.into_iter().take(2) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let content = get_page(client, Provider::Genius, &item.source).and_then(|page| {
                    page.map(|page| lyrics_text(&Html::parse_document(&page), Provider::Genius))
                        .transpose()
                });
                match content {
                    Ok(Some(plain)) => {
                        item.plain = plain;
                        found.push(item);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        warnings.push(error.to_string());
                        break;
                    }
                }
            }
            if !warnings.is_empty() || found.iter().any(|item| item.score >= 900) {
                break;
            }
        }
        Ok(())
    })();
    if let Err(error) = operation {
        warnings.push(error.to_string());
    }
    warnings.sort();
    warnings.dedup();
    Ok(SearchResult {
        matches: found,
        warnings,
    })
}

fn musixmatch(
    client: &Client,
    data: &Path,
    query: &Query,
    guesses: &[(String, String)],
    cancel: &AtomicBool,
) -> Result<SearchResult> {
    let key = secrets::get(data, Provider::Musixmatch.key_id()?)?
        .context("Добавьте ключ Musixmatch в разделе Источники.")?;
    let mut found = vec![];
    let mut seen = HashSet::new();
    let operation = (|| -> Result<()> {
        for (artist, title) in guesses.iter().take(3) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let value = request_json(
                client
                    .get("https://api.musixmatch.com/ws/1.1/track.search")
                    .query(&[
                        ("q_artist", artist.as_str()),
                        ("q_track", title.as_str()),
                        ("f_has_lyrics", "1"),
                        ("page_size", "5"),
                        ("apikey", &key),
                    ]),
            )?;
            let body = musix_body(&value)?;
            let tracks = body["track_list"]
                .as_array()
                .context("Musixmatch вернул неизвестный ответ поиска.")?;
            for track in tracks.iter().take(5) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let track = &track["track"];
                let Some(id) = track["track_id"].as_u64() else {
                    continue;
                };
                if !seen.insert(id) {
                    continue;
                }
                let mut item = lyric(
                    Provider::Musixmatch,
                    track["artist_name"].as_str().unwrap_or(""),
                    track["track_name"].as_str().unwrap_or(""),
                    track["track_share_url"]
                        .as_str()
                        .unwrap_or("https://www.musixmatch.com/")
                        .into(),
                    String::new(),
                );
                item.album = track["album_name"].as_str().unwrap_or("").into();
                item.score = rank(&item, query, guesses);
                if item.score < 600 {
                    continue;
                }
                let value = request_json(
                    client
                        .get("https://api.musixmatch.com/ws/1.1/track.lyrics.get")
                        .query(&[("track_id", id.to_string()), ("apikey", key.clone())]),
                )?;
                let body = musix_body(&value)?;
                ensure!(
                    body["lyrics"]["restricted"].as_u64().unwrap_or(0) == 0,
                    "Текст недоступен для этого ключа или региона."
                );
                item.plain = body["lyrics"]["lyrics_body"]
                    .as_str()
                    .unwrap_or("")
                    .trim()
                    .into();
                if item.plain.is_empty() {
                    continue;
                }
                // Keep the provider's attribution and any notice about partial access.
                if let Some(copyright) = body["lyrics"]["lyrics_copyright"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                {
                    item.plain.push_str("\n\n");
                    item.plain.push_str(copyright);
                }
                found.push(item);
                if found.len() >= 2 {
                    return Ok(());
                }
            }
            if found.iter().any(|item| item.score >= 900) {
                break;
            }
        }
        Ok(())
    })();
    Ok(SearchResult {
        matches: found,
        warnings: operation.err().map(|e| e.to_string()).into_iter().collect(),
    })
}

fn musix_body(value: &Value) -> Result<&Value> {
    let status = value["message"]["header"]["status_code"]
        .as_u64()
        .unwrap_or(0);
    ensure!(
        status == 200,
        "{} (Musixmatch {status}).",
        match status {
            401 => "Ключ Musixmatch не принят",
            402 | 403 => "Для этого текста нужен другой доступ Musixmatch",
            404 => "Текст не найден",
            429 => "Исчерпан лимит запросов",
            _ => "Musixmatch временно недоступен",
        }
    );
    Ok(&value["message"]["body"])
}

fn latin_lowercase(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            _ => c,
        })
        .collect()
}

fn slug(value: &str, separator: &str) -> String {
    normalized(&latin_lowercase(value))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(separator)
}

fn elyrics_slug(value: &str) -> String {
    latin_lowercase(value)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "_,&-()".contains(c) {
                c
            } else if c.is_whitespace() {
                ' '
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn pages(
    client: &Client,
    provider: Provider,
    query: &Query,
    guesses: &[(String, String)],
    cancel: &AtomicBool,
) -> Result<Vec<Lyric>> {
    let mut seen = HashSet::new();
    for (artist, title) in guesses
        .iter()
        .filter(|(a, t)| !a.is_empty() && !t.is_empty())
        .take(3)
    {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let a = slug(artist, "-");
        let t = slug(title, "-");
        if a.is_empty() || t.is_empty() {
            continue;
        }
        let url = match provider {
            Provider::Azlyrics => format!(
                "https://www.azlyrics.com/lyrics/{}/{}.html",
                slug(artist, ""),
                slug(title, "")
            ),
            Provider::Songlyrics => format!("https://www.songlyrics.com/{a}/{t}-lyrics/"),
            Provider::Elyrics => format!(
                "https://www.elyrics.net/read/{}/{}-lyrics/{}-lyrics.html",
                a.chars().next().unwrap(),
                elyrics_slug(artist),
                elyrics_slug(title)
            ),
            Provider::Letras => format!("https://www.letras.mus.br/{a}/{t}/"),
            _ => unreachable!(),
        };
        if !seen.insert(url.clone()) {
            continue;
        }
        let Some(page) = get_page(client, provider, &url)? else {
            continue;
        };
        let document = Html::parse_document(&page);
        let plain = lyrics_text(&document, provider)?;
        let lower = plain.to_lowercase();
        if lower.contains("we do not have the lyrics")
            || lower.contains("we don't have the lyrics")
            || lower.contains("lyrics are not available")
        {
            continue;
        }
        let mut item = lyric(provider, artist, title, url, plain);
        item.score = rank(&item, query, guesses);
        // Direct URL guesses have no independent artist/title verification.
        // Require the user to choose them rather than opening a possible collision.
        item.score = item.score.min(850);
        return Ok(vec![item]);
    }
    Ok(vec![])
}

fn yandex(data: &Path, query: &Query, cancel: &AtomicBool) -> Result<SearchResult> {
    let connection = services::list(data)?
        .into_iter()
        .find(|c| c.provider == services::Provider::Yandex && c.enabled && c.authenticated)
        .context("Подключите Яндекс Музыку в разделе Сервисы.")?;
    let guesses = candidates(query);
    let mut seen = HashSet::new();
    let mut matches = vec![];
    let operation = (|| -> Result<()> {
        for (artist, title) in guesses.iter().take(3) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let tracks = services::search(
                data,
                &connection,
                Some(&format!("{artist} {title}")),
                0,
                cancel,
            )?;
            let mut pending = vec![];
            for track in tracks.into_iter().take(20) {
                if !seen.insert(track.id.clone()) {
                    continue;
                }
                let mut item = lyric(
                    Provider::Yandex,
                    &track.artist,
                    &track.title,
                    format!(
                        "https://music.yandex.ru/track/{}",
                        track.id.split(':').next().unwrap_or(&track.id)
                    ),
                    String::new(),
                );
                item.album = track.album;
                item.duration = Some(track.duration_ms as f64 / 1000.0);
                item.score = rank(&item, query, &guesses);
                if item.score >= 600 {
                    pending.push((track.id, item));
                }
            }
            pending.sort_by_key(|(_, item)| std::cmp::Reverse(item.score));
            for (id, mut item) in pending.into_iter().take(2) {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let value = services::lyrics(data, &connection, &id, cancel)?;
                item.plain = value["plain"].as_str().unwrap_or("").into();
                item.synced = value["synced"].as_str().unwrap_or("").into();
                if !item.content().is_empty() {
                    matches.push(item);
                }
            }
            if matches.iter().any(|item| item.score >= 900) {
                break;
            }
        }
        Ok(())
    })();
    Ok(SearchResult {
        matches,
        warnings: operation.err().map(|e| e.to_string()).into_iter().collect(),
    })
}
