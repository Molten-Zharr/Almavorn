mod spotify;
use crate::{network, process, secrets};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub use spotify::{Device, RemoteState};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    Yandex,
    Spotify,
    YouTube,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Yandex => "Яндекс Музыка",
            Self::Spotify => "Spotify",
            Self::YouTube => "YouTube Music",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub provider: Provider,
    pub client_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub authenticated: bool,
}
impl Connection {
    pub fn new(provider: Provider) -> Self {
        Self {
            id: format!("{:032x}", fastrand::u128(..)),
            name: provider.name().into(),
            provider,
            client_id: String::new(),
            enabled: true,
            authenticated: false,
        }
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.id.len() == 32 && self.id.bytes().all(|c| c.is_ascii_hexdigit()),
            "Повреждена запись подключения."
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 100,
            "Введите название до 100 символов."
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub available: bool,
}
#[derive(Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub url: String,
    pub expires_in: u64,
    pub interval: u64,
}
pub struct Media {
    pub url: String,
    pub headers: Vec<(String, String)>,
}
fn path(data: &Path, id: &str) -> Result<PathBuf> {
    ensure!(
        id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()),
        "Повреждена запись подключения."
    );
    Ok(data.join("services").join(format!("{id}.json")))
}
pub fn list(data: &Path) -> Result<Vec<Connection>> {
    let directory = data.join("services");
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for entry in std::fs::read_dir(directory)?.take(100) {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == "json") {
            let row: Connection = network::load_json(&path)?;
            row.validate()?;
            result.push(row);
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}
pub fn save(data: &Path, connection: &Connection) -> Result<()> {
    connection.validate()?;
    let target = path(data, &connection.id)?;
    if target.exists() {
        let old: Connection = network::load_json(&target)?;
        ensure!(
            old.provider == connection.provider,
            "Сервис подключения нельзя изменить."
        );
        if old.client_id != connection.client_id {
            logout(data, &old)?;
            let mut changed = connection.clone();
            changed.authenticated = false;
            return network::save_json(&target, &changed);
        }
        let mut value = connection.clone();
        value.authenticated = old.authenticated;
        return network::save_json(&target, &value);
    }
    network::save_json(&target, connection)
}
pub fn remove(data: &Path, connection: &Connection) -> Result<()> {
    logout(data, connection)?;
    std::fs::remove_file(path(data, &connection.id)?)?;
    Ok(())
}
pub fn logout(data: &Path, connection: &Connection) -> Result<()> {
    if connection.authenticated {
        secrets::remove(data, &format!("service-{}", connection.id))?;
    }
    let mut row = connection.clone();
    row.authenticated = false;
    network::save_json(&path(data, &row.id)?, &row)
}
fn secret(data: &Path, connection: &Connection) -> Result<Option<Value>> {
    if !connection.authenticated {
        return Ok(None);
    }
    secrets::get(data, &format!("service-{}", connection.id))?
        .map(|value| {
            serde_json::from_str(&value).context("Сохраненный вход поврежден. Войдите снова.")
        })
        .transpose()
}
fn save_secret(data: &Path, connection: &Connection, value: &Value) -> Result<()> {
    secrets::set(
        data,
        &format!("service-{}", connection.id),
        &serde_json::to_string(value)?,
    )?;
    let mut row = connection.clone();
    row.authenticated = true;
    network::save_json(&path(data, &row.id)?, &row)
}
fn bridge(
    data: &Path,
    connection: &Connection,
    mut request: Value,
    cancel: &AtomicBool,
) -> Result<Value> {
    ensure!(connection.enabled, "Включите подключение к сервису.");
    let python = adapter_directory()?.join("bin/python");
    ensure!(python.is_file(), "Нажмите Установить поддержку сервисов.");
    request["provider"] = json!(connection.provider);
    request["secret"] = secret(data, connection)?.unwrap_or(Value::Null);
    let mut command = Command::new(python);
    command.args(["-I", "-c", include_str!("bridge.py")]);
    let output = process::run(
        command,
        Some(serde_json::to_vec(&request)?),
        cancel,
        Duration::from_secs(if request["operation"] == "lyrics" { 35 } else { 120 }),
        8 * 1024 * 1024,
    )?;
    ensure!(
        output.success,
        "Не удалось запустить поддержку сервиса. Повторите установку компонентов."
    );
    let value: Value =
        serde_json::from_slice(&output.stdout).context("Сервис вернул неизвестный ответ.")?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        bail!(match error {
            "auth" => "Вход истек. Подключите аккаунт снова.",
            "access" => "Запись недоступна для этого аккаунта или региона. Проверьте подписку.",
            "limit" => "Сервис ограничил запросы. Повторите позже.",
            "network" => "Нет ответа от сервиса. Проверьте соединение.",
            "install" => "Установите поддержку сервисов.",
            _ => "Сервис не выполнил запрос. Проверьте вход или обновите поддержку сервисов.",
        });
    }
    ensure!(!cancel.load(Ordering::Relaxed), "Операция отменена.");
    let mut value = value.get("ok").cloned().context("Пустой ответ сервиса.")?;
    if let Some(secret) = value.as_object_mut().and_then(|o| o.remove("secret")) {
        save_secret(data, connection, &secret)?;
    }
    Ok(value)
}
pub fn search(
    data: &Path,
    connection: &Connection,
    query: Option<&str>,
    page: u32,
    cancel: &AtomicBool,
) -> Result<Vec<Track>> {
    ensure!(page < 1000, "Достигнут конец списка.");
    if connection.provider == Provider::Spotify {
        return spotify::search(data, connection, query, page, cancel);
    }
    let response = bridge(
        data,
        connection,
        json!({"operation":if query.is_some(){"search"}else{"library"},"query":query,"page":page}),
        cancel,
    )?;
    serde_json::from_value(response["tracks"].clone())
        .context("Не удалось прочитать список композиций.")
}
pub fn device_code(
    data: &Path,
    connection: &Connection,
    cancel: &AtomicBool,
) -> Result<DeviceCode> {
    serde_json::from_value(bridge(
        data,
        connection,
        json!({"operation":"device"}),
        cancel,
    )?)
    .context("Не удалось начать вход.")
}
pub(crate) fn lyrics(
    data: &Path,
    connection: &Connection,
    track: &str,
    cancel: &AtomicBool,
) -> Result<Value> {
    ensure!(connection.provider == Provider::Yandex, "Этот сервис не подключен к поиску текстов.");
    ensure!(
        !track.is_empty() && track.len() <= 100
            && track.chars().all(|c| c.is_ascii_digit() || c == ':'),
        "Неизвестная запись Яндекс Музыки."
    );
    bridge(data, connection, json!({"operation":"lyrics", "track_id":track.split(':').next().unwrap_or(track)}), cancel)
}
pub fn device_poll(
    data: &Path,
    connection: &Connection,
    code: &str,
    cancel: &AtomicBool,
) -> Result<bool> {
    Ok(!bridge(
        data,
        connection,
        json!({"operation":"poll","device_code":code}),
        cancel,
    )?
    .get("pending")
    .and_then(Value::as_bool)
    .unwrap_or(false))
}
pub fn youtube_login(
    data: &Path,
    connection: &Connection,
    headers: &str,
    cancel: &AtomicBool,
) -> Result<()> {
    bridge(
        data,
        connection,
        json!({"operation":"headers","headers":headers}),
        cancel,
    )?;
    Ok(())
}
pub fn spotify_login(
    data: &Path,
    connection: &Connection,
    cancel: &AtomicBool,
    link: impl Fn(String),
) -> Result<()> {
    spotify::login(data, connection, cancel, link)
}
pub fn devices(data: &Path, connection: &Connection, cancel: &AtomicBool) -> Result<Vec<Device>> {
    spotify::devices(data, connection, cancel)
}
pub fn remote(
    data: &Path,
    connection: &Connection,
    device: Option<&str>,
    operation: &str,
    track: Option<&str>,
    number: u64,
    cancel: &AtomicBool,
) -> Result<RemoteState> {
    spotify::control(data, connection, device, operation, track, number, cancel)
}
pub fn resolve(
    data: &Path,
    connection: &Connection,
    track: &Track,
    cancel: &AtomicBool,
) -> Result<Media> {
    ensure!(track.available, "Композиция недоступна для этого аккаунта.");
    if connection.provider == Provider::Yandex {
        let value = bridge(
            data,
            connection,
            json!({"operation":"resolve","track_id":track.id}),
            cancel,
        )?;
        let url = value["url"].as_str().context("Сервис не вернул аудио.")?;
        network::public_url(url)?;
        return Ok(Media {
            url: url.into(),
            headers: vec![],
        });
    }
    ensure!(
        connection.provider == Provider::YouTube,
        "Spotify воспроизводится на устройстве Spotify Connect."
    );
    ensure!(
        track.id.len() <= 100
            && track
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')),
        "Неизвестная запись YouTube."
    );
    let mut cookies = None;
    if let Some(secret) = secret(data, connection)? {
        use std::io::Write;
        if let Some(value) = secret
            .as_object()
            .and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case("cookie")))
            .and_then(|(_, v)| v.as_str())
        {
            let mut file = tempfile::NamedTempFile::new()?;
            writeln!(file, "# Netscape HTTP Cookie File")?;
            for item in value.split(';') {
                if let Some((name, value)) = item.trim().split_once('=') {
                    ensure!(
                        !name.chars().chain(value.chars()).any(char::is_control),
                        "Заголовки входа повреждены."
                    );
                    writeln!(file, ".youtube.com\tTRUE\t/\tTRUE\t0\t{name}\t{value}")?;
                }
            }
            file.flush()?;
            cookies = Some(file);
        }
    }
    let mut command = crate::downloads::command(data)?;
    if let Some(file) = &cookies {
        command.arg("--cookies").arg(file.path());
    }
    command
        .args([
            "--skip-download",
            "--dump-single-json",
            "--format",
            "bestaudio[ext=m4a]/bestaudio[acodec=vorbis]/bestaudio",
            "--",
        ])
        .arg(format!("https://music.youtube.com/watch?v={}", track.id));
    let output = process::run(
        command,
        None,
        cancel,
        Duration::from_secs(120),
        8 * 1024 * 1024,
    )?;
    ensure!(
        output.success,
        "Не удалось открыть аудио YouTube Music. Обновите компоненты загрузки; запись может требовать отдельного доступа."
    );
    let value: Value = serde_json::from_slice(&output.stdout)?;
    ensure!(
        !value["has_drm"].as_bool().unwrap_or(false),
        "Эта запись защищена и не поддерживается."
    );
    let url = value["url"].as_str().context("Сервис не вернул аудио.")?;
    network::public_url(url)?;
    let headers = value["http_headers"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.into())))
                .collect()
        })
        .unwrap_or_default();
    Ok(Media {
        url: url.into(),
        headers,
    })
}
fn adapter_directory() -> Result<PathBuf> {
    use sha2::Digest;
    let digest = format!(
        "{:x}",
        sha2::Sha256::digest(include_bytes!("requirements.lock"))
    );
    Ok(crate::components::directory()?.join(format!("services-{}", &digest[..16])))
}
pub fn install(_data: &Path, cancel: &AtomicBool) -> Result<()> {
    let root = crate::components::directory()?;
    std::fs::create_dir_all(&root)?;
    crate::components::validate(&root)?;
    let target = adapter_directory()?;
    if target.join("installed").is_file() {
        return Ok(());
    }
    ensure!(
        !target.exists(),
        "Папка поддержки сервисов повреждена. Переименуйте {} и повторите установку.",
        target.display()
    );
    let stage = tempfile::Builder::new()
        .prefix(".services-")
        .tempdir_in(&root)?;
    let venv = stage.path().join("venv");
    let mut cmd = Command::new("python3");
    cmd.args(["-I", "-m", "venv"]).arg(&venv);
    let output = process::run(cmd, None, cancel, Duration::from_secs(120), 1024 * 1024)?;
    ensure!(
        output.success,
        "Установите Python 3.10 или новее с поддержкой venv, затем повторите действие."
    );
    let requirements = stage.path().join("requirements.lock");
    std::fs::write(&requirements, include_bytes!("requirements.lock"))?;
    let mut cmd = Command::new(venv.join("bin/python"));
    cmd.args([
        "-I",
        "-m",
        "pip",
        "install",
        "--isolated",
        "--disable-pip-version-check",
        "--require-hashes",
        "--only-binary=:all:",
        "--index-url",
        "https://pypi.org/simple",
        "--requirement",
    ])
    .arg(requirements);
    let output = process::run(cmd, None, cancel, Duration::from_secs(600), 1024 * 1024)?;
    ensure!(
        output.success,
        "Не удалось установить поддержку сервисов. Проверьте соединение и версию Python."
    );
    ensure!(!cancel.load(Ordering::Relaxed), "Установка отменена.");
    std::fs::write(venv.join("installed"), b"1")?;
    crate::tags::move_noreplace(&venv, &target)?;
    Ok(())
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub count: u64,
}
pub fn library(
    data: &Path,
    connection: &Connection,
    kind: &str,
    playlist: Option<&str>,
    page: u32,
    cancel: &AtomicBool,
) -> Result<Value> {
    ensure!(page < 1000, "Достигнут конец списка.");
    if connection.provider == Provider::Spotify {
        return spotify::library(data, connection, kind, playlist, page, cancel);
    }
    bridge(
        data,
        connection,
        json!({"operation":kind,"playlist":playlist,"page":page}),
        cancel,
    )
}
