use super::{Connection, Track, save_secret, secret};
use crate::network;
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::SPOTIFY_REDIRECT as REDIRECT;
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn random() -> Result<String> {
    let mut data = [0u8; 32];
    getrandom::fill(&mut data)
        .map_err(|_| anyhow::anyhow!("Не удалось создать защищенный код входа."))?;
    Ok(URL_SAFE_NO_PAD.encode(data))
}
fn response(response: reqwest::blocking::Response) -> Result<Value> {
    match response.status().as_u16() {
        204 => return Ok(Value::Null),
        401 => bail!("Вход Spotify истек. Подключите аккаунт снова."),
        403 => bail!(
            "Spotify отклонил действие. Проверьте Premium и доступ аккаунта к приложению Spotify."
        ),
        404 => bail!("Откройте Spotify на нужном устройстве и выберите его в списке устройств."),
        429 => {
            let seconds = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(30);
            bail!("Spotify ограничил запросы. Повторите через {seconds} с.");
        }
        _ => {}
    }
    network::json(response)
}
fn token_response(
    data: &Path,
    connection: &Connection,
    value: Value,
    previous: Option<&Value>,
) -> Result<String> {
    let access = value["access_token"]
        .as_str()
        .context("Spotify не подтвердил вход.")?
        .to_owned();
    let refresh = value
        .get("refresh_token")
        .or_else(|| previous.and_then(|p| p.get("refresh_token")))
        .cloned()
        .unwrap_or(Value::Null);
    save_secret(
        data,
        connection,
        &json!({"access_token":access,"refresh_token":refresh,"expires_at":now()+value["expires_in"].as_u64().unwrap_or(3600)}),
    )?;
    Ok(access)
}
fn access(data: &Path, connection: &Connection, cancel: &AtomicBool) -> Result<String> {
    ensure!(connection.enabled, "Включите подключение Spotify.");
    let secret = secret(data, connection)?.context("Подключите аккаунт Spotify.")?;
    if secret["expires_at"].as_u64().unwrap_or(0) > now() + 60 {
        return secret["access_token"]
            .as_str()
            .map(str::to_owned)
            .context("Подключите Spotify снова.");
    }
    let refresh = secret["refresh_token"]
        .as_str()
        .context("Подключите Spotify снова.")?;
    let value = request(cancel, |client| {
        client
            .post("https://accounts.spotify.com/api/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", connection.client_id.as_str()),
            ])
    })?;
    ensure!(!cancel.load(Ordering::Relaxed), "Операция отменена.");
    token_response(data, connection, value, Some(&secret))
}
fn request(
    cancel: &AtomicBool,
    build: impl FnOnce(&reqwest::Client) -> reqwest::RequestBuilder,
) -> Result<Value> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client=reqwest::Client::builder().connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(12)).build()?;
        tokio::select!{
            result=async {
                let mut response=build(&client).send().await.map_err(|_|anyhow::anyhow!("Нет ответа от Spotify. Проверьте соединение."))?;
                match response.status().as_u16(){204=>return Ok(Value::Null),401=>bail!("Вход Spotify истек. Подключите аккаунт снова."),403=>bail!("Spotify отклонил действие. Проверьте Premium и доступ аккаунта к приложению Spotify."),404=>bail!("Откройте Spotify на нужном устройстве и выберите его в списке устройств."),429=>{let seconds=response.headers().get("retry-after").and_then(|v|v.to_str().ok()).and_then(|s|s.parse::<u64>().ok()).unwrap_or(30);bail!("Spotify ограничил запросы. Повторите через {seconds} с.");},_=>{}}
                ensure!(response.status().is_success(),"Spotify отклонил запрос (HTTP {}).",response.status().as_u16());
                let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await.map_err(|_|anyhow::anyhow!("Ответ Spotify прерван."))?{ensure!(bytes.len()+chunk.len()<=4*1024*1024,"Ответ Spotify слишком большой.");bytes.extend_from_slice(&chunk);}
                serde_json::from_slice(&bytes).context("Не удалось прочитать ответ Spotify.")
            }=>result,
            _=async {while !cancel.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(50)).await;}}=>bail!("Операция отменена."),
        }
    })
}
fn api(
    data: &Path,
    connection: &Connection,
    method: reqwest::Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
    cancel: &AtomicBool,
) -> Result<Value> {
    let token = access(data, connection, cancel)?;
    ensure!(!cancel.load(Ordering::Relaxed), "Операция отменена.");
    request(cancel, |client| {
        let mut request = client
            .request(method, format!("https://api.spotify.com/v1/{path}"))
            .bearer_auth(token)
            .query(query);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request
    })
}
pub(super) fn login(
    data: &Path,
    connection: &Connection,
    cancel: &AtomicBool,
    link: impl Fn(String),
) -> Result<()> {
    ensure!(
        connection.client_id.len() == 32
            && connection.client_id.bytes().all(|c| c.is_ascii_hexdigit()),
        "Укажите Client ID из приложения Spotify for Developers."
    );
    let listener = TcpListener::bind("127.0.0.1:43827")
        .context("Вход Spotify уже открыт. Завершите его или повторите позже.")?;
    listener.set_nonblocking(true)?;
    let verifier = random()?;
    let state = random()?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = reqwest::Url::parse("https://accounts.spotify.com/authorize")?;
    url.query_pairs_mut().extend_pairs([("client_id",connection.client_id.as_str()),("response_type","code"),("redirect_uri",REDIRECT),("scope","user-read-private user-library-read user-read-playback-state user-modify-playback-state"),("state",state.as_str()),("code_challenge_method","S256"),("code_challenge",challenge.as_str())]);
    link(url.to_string());
    let deadline = Instant::now() + Duration::from_secs(300);
    let code = loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Вход отменен.");
        ensure!(
            Instant::now() < deadline,
            "Время входа истекло. Повторите действие."
        );
        let (mut stream, _) = match listener.accept() {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while request.len() < 8192 {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                break;
            }
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&request);
        let Some(target) = text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("GET "))
            .and_then(|line| line.split_whitespace().next())
        else {
            continue;
        };
        let Ok(url) = reqwest::Url::parse(&format!("http://127.0.0.1:43827{target}")) else {
            continue;
        };
        let params = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        if url.path() != "/callback" || params.get("state").map(|s| s.as_ref()) != Some(&state) {
            let _ = stream.write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
            continue;
        }
        let body = "You can return to Almavorn.";
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        ensure!(!params.contains_key("error"), "Вход в Spotify отменен.");
        break params
            .get("code")
            .context("Spotify не передал код входа.")?
            .to_string();
    };
    ensure!(!cancel.load(Ordering::Relaxed), "Вход отменен.");
    let value = response(
        network::client()?
            .post("https://accounts.spotify.com/api/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", REDIRECT),
                ("client_id", connection.client_id.as_str()),
                ("code_verifier", verifier.as_str()),
            ])
            .send()
            .map_err(|_| anyhow::anyhow!("Не удалось завершить вход Spotify."))?,
    )?;
    ensure!(!cancel.load(Ordering::Relaxed), "Вход отменен.");
    token_response(data, connection, value, None)?;
    Ok(())
}
fn track(value: &Value) -> Option<Track> {
    Some(Track {
        id: value["uri"].as_str()?.into(),
        title: value["name"].as_str().unwrap_or("").into(),
        artist: value["artists"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        album: value["album"]["name"].as_str().unwrap_or("").into(),
        duration_ms: value["duration_ms"].as_u64().unwrap_or(0),
        available: value["is_playable"].as_bool().unwrap_or(true),
    })
}
pub(super) fn search(
    data: &Path,
    connection: &Connection,
    query: Option<&str>,
    page: u32,
    cancel: &AtomicBool,
) -> Result<Vec<Track>> {
    // Current development-mode search caps each page at ten items.
    let value = if let Some(query) = query {
        api(
            data,
            connection,
            reqwest::Method::GET,
            "search",
            &[
                ("q", query.into()),
                ("type", "track".into()),
                ("limit", "10".into()),
                ("offset", (page * 10).to_string()),
            ],
            None,
            cancel,
        )?
    } else {
        api(
            data,
            connection,
            reqwest::Method::GET,
            "me/tracks",
            &[("limit", "50".into()), ("offset", (page * 50).to_string())],
            None,
            cancel,
        )?
    };
    let items = if query.is_some() {
        value["tracks"]["items"].as_array()
    } else {
        value["items"].as_array()
    };
    Ok(items
        .into_iter()
        .flatten()
        .filter_map(|row| track(if query.is_some() { row } else { &row["track"] }))
        .collect())
}
#[derive(Clone, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    pub is_active: bool,
    pub is_restricted: bool,
    #[serde(default)]
    pub supports_volume: bool,
}
#[derive(Clone, Default)]
pub struct RemoteState {
    pub playing: bool,
    pub progress_ms: u64,
    pub track: Option<Track>,
    pub device: String,
    pub volume: Option<u64>,
}
pub(super) fn devices(
    data: &Path,
    connection: &Connection,
    cancel: &AtomicBool,
) -> Result<Vec<Device>> {
    let value = api(
        data,
        connection,
        reqwest::Method::GET,
        "me/player/devices",
        &[],
        None,
        cancel,
    )?;
    serde_json::from_value(value["devices"].clone())
        .context("Не удалось прочитать устройства Spotify.")
}
pub(super) fn control(
    data: &Path,
    connection: &Connection,
    device: Option<&str>,
    operation: &str,
    track_id: Option<&str>,
    number: u64,
    cancel: &AtomicBool,
) -> Result<RemoteState> {
    let mut query = vec![];
    if let Some(device) = device {
        query.push(("device_id", device.to_string()));
    }
    let (method, path, body) = match operation {
        "play" => {
            let body = if let Some(track) = track_id {
                ensure!(
                    track.starts_with("spotify:track:") && track.len() < 100,
                    "Неизвестная композиция Spotify."
                );
                Some(json!({"uris":[track]}))
            } else {
                None
            };
            (reqwest::Method::PUT, "me/player/play", body)
        }
        "pause" => (reqwest::Method::PUT, "me/player/pause", None),
        "next" => (reqwest::Method::POST, "me/player/next", None),
        "previous" => (reqwest::Method::POST, "me/player/previous", None),
        "seek" => {
            query.push(("position_ms", number.to_string()));
            (reqwest::Method::PUT, "me/player/seek", None)
        }
        "volume" => {
            query.push(("volume_percent", number.min(100).to_string()));
            (reqwest::Method::PUT, "me/player/volume", None)
        }
        "state" => (reqwest::Method::GET, "me/player", None),
        _ => bail!("Неизвестное действие Spotify."),
    };
    let mut value = api(data, connection, method, path, &query, body, cancel)?;
    if operation != "state" {
        std::thread::sleep(Duration::from_millis(250));
        value = api(
            data,
            connection,
            reqwest::Method::GET,
            "me/player",
            &[],
            None,
            cancel,
        )?;
    }
    Ok(RemoteState {
        playing: value["is_playing"].as_bool().unwrap_or(false),
        progress_ms: value["progress_ms"].as_u64().unwrap_or(0),
        track: track(&value["item"]),
        device: value["device"]["name"].as_str().unwrap_or("").into(),
        volume: value["device"]["volume_percent"].as_u64(),
    })
}

pub(super) fn library(
    data: &Path,
    connection: &Connection,
    kind: &str,
    playlist: Option<&str>,
    page: u32,
    cancel: &AtomicBool,
) -> Result<Value> {
    if matches!(kind, "library" | "liked") {
        return Ok(json!({"tracks":search(data,connection,None,page,cancel)?}));
    }
    let path = if kind == "playlists" {
        "me/playlists".into()
    } else {
        let id = playlist.context("Выберите плейлист.")?;
        ensure!(
            id.len() <= 100 && id.bytes().all(|c| c.is_ascii_alphanumeric()),
            "Неизвестный плейлист Spotify."
        );
        format!("playlists/{id}/items")
    };
    let value = api(
        data,
        connection,
        reqwest::Method::GET,
        &path,
        &[("limit", "50".into()), ("offset", (page * 50).to_string())],
        None,
        cancel,
    )?;
    let rows = value["items"].as_array().into_iter().flatten();
    if kind == "playlists" {
        Ok(
            json!({"playlists":rows.filter_map(|p|Some(super::Playlist{id:p["id"].as_str()?.into(),name:p["name"].as_str().unwrap_or("").into(),count:p["items"]["total"].as_u64().or_else(||p["tracks"]["total"].as_u64()).unwrap_or(0)})).collect::<Vec<_>>()}),
        )
    } else {
        Ok(
            json!({"tracks":rows.filter_map(|row|track(row.get("item").or_else(||row.get("track")).unwrap_or(&Value::Null))).collect::<Vec<_>>()}),
        )
    }
}
