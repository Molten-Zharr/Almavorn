use crate::{network, process, secrets};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Engine {
    #[default]
    YtDlp,
    Cobalt,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub directory: PathBuf,
    pub engine: Engine,
    pub cobalt_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            directory: dirs::audio_dir()
                .or_else(dirs::download_dir)
                .unwrap_or_else(std::env::temp_dir)
                .join("Almavorn"),
            engine: Engine::YtDlp,
            cobalt_url: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Queued,
    Running,
    Publishing,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: String,
    pub url: String,
    pub name: String,
    pub folder: PathBuf,
    pub engine: Engine,
    pub status: Status,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
    #[serde(default)]
    pub notice: Option<String>,
    pub imported: bool,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub expected_digest: Option<String>,
    #[serde(default)]
    pub expected_size: Option<u64>,
}
impl Download {
    pub fn new(url: String, name: String, folder: PathBuf, engine: Engine) -> Result<Self> {
        network::public_url(&url)?;
        let folder = destination_folder(&folder)?;
        Ok(Self {
            id: format!("{:032x}", fastrand::u128(..)),
            url,
            name,
            folder,
            engine,
            status: Status::Queued,
            path: None,
            error: None,
            notice: None,
            imported: false,
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            expected_digest: None,
            expected_size: None,
        })
    }
    fn published(&self) -> bool {
        self.path.as_ref().is_some_and(|path| {
            std::fs::symlink_metadata(path)
                .is_ok_and(|m| m.is_file() && Some(m.len()) == self.expected_size)
                && self.expected_digest.as_ref().is_some_and(|expected| {
                    network::file_digest(path).is_ok_and(|digest| &digest == expected)
                })
        })
    }
}

fn downloads_folder() -> Result<PathBuf> {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .context("Не найдена папка Загрузки. Укажите папку в настройках загрузок.")
}

pub fn destination_folder(value: &Path) -> Result<PathBuf> {
    if value.as_os_str().is_empty() {
        return downloads_folder();
    }
    ensure!(value.is_absolute(), "Укажите полный путь к папке загрузки.");
    Ok(value.to_owned())
}

fn staging_folder(requested: &Path, cancel: &AtomicBool) -> Result<(PathBuf, tempfile::TempDir)> {
    let mut candidates = vec![destination_folder(requested)?];
    if let Ok(downloads) = downloads_folder() {
        for folder in [downloads.join("Almavorn"), downloads] {
            if !candidates.contains(&folder) {
                candidates.push(folder);
            }
        }
    }
    let mut errors = Vec::new();
    for folder in candidates {
        ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена.");
        let prepare = || -> Result<_> {
            std::fs::create_dir_all(&folder)?;
            let folder = folder.canonicalize()?;
            // Creating staging checks that this directory is actually writable.
            let staging = tempfile::Builder::new()
                .prefix(".almavorn-download-")
                .tempdir_in(&folder)?;
            Ok((folder, staging))
        };
        match prepare() {
            Ok(prepared) => return Ok(prepared),
            Err(error) => errors.push(format!("{}: {error:#}", folder.display())),
        }
    }
    bail!("Не удалось создать папку для музыки. Выберите доступную папку в настройках загрузок.\n{}", errors.join("\n"))
}

#[derive(Clone)]
pub struct SearchHit {
    pub title: String,
    pub artist: String,
    pub url: String,
    pub duration: Option<f64>,
}

pub fn config(data: &Path) -> Result<Config> {
    let path = data.join("downloads.json");
    if path.try_exists()? {
        network::load_json(&path)
    } else {
        Ok(Config::default())
    }
}
fn cobalt_key(url: &reqwest::Url) -> String {
    use sha2::Digest;
    format!(
        "cobalt-{:x}",
        sha2::Sha256::digest(url.origin().ascii_serialization().as_bytes())
    )
}
fn cobalt_endpoint(value: &str) -> Result<reqwest::Url> {
    let url = network::public_url(value)?;
    ensure!(
        url.scheme() == "https"
            || matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
        "Для Cobalt нужен HTTPS."
    );
    Ok(url)
}
pub fn save_config(data: &Path, config: &Config, key: Option<&str>) -> Result<()> {
    ensure!(
        config.directory.is_absolute(),
        "Укажите полный путь к папке загрузки."
    );
    if !config.cobalt_url.is_empty() {
        let url = network::public_url(&config.cobalt_url)?;
        ensure!(
            url.scheme() == "https"
                || matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1")),
            "Для Cobalt нужен HTTPS."
        );
    }
    if let Some(key) = key {
        let endpoint = cobalt_endpoint(&config.cobalt_url)?;
        let account = cobalt_key(&endpoint);
        if key.is_empty() {
            secrets::remove(data, &account)?;
        } else {
            secrets::set(data, &account, key)?;
        }
    }
    network::save_json(&data.join("downloads.json"), config)
}
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn save(data: &Path, download: &Download) -> Result<()> {
    ensure!(valid_id(&download.id), "Неверный номер загрузки.");
    network::save_json(
        &data.join("downloads").join(format!("{}.json", download.id)),
        download,
    )
}
pub fn remove(data: &Path, id: &str) -> Result<()> {
    ensure!(valid_id(id), "Неверный номер загрузки.");
    std::fs::remove_file(data.join("downloads").join(format!("{id}.json")))?;
    Ok(())
}
pub fn list(data: &Path) -> Result<Vec<Download>> {
    let directory = data.join("downloads");
    if !directory.try_exists()? {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let mut row: Download = match network::load_json(&path) {
            Ok(row) => row,
            Err(_) => {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if !valid_id(id) {
                    continue;
                }
                Download {
                    id: id.into(),
                    url: String::new(),
                    name: "Поврежденная запись загрузки".into(),
                    folder: Config::default().directory,
                    engine: Engine::YtDlp,
                    status: Status::Failed,
                    path: None,
                    error: Some(
                        "Запись очереди повреждена. Измените ссылку и папку или удалите запись."
                            .into(),
                    ),
                    notice: None,
                    imported: false,
                    created: 0,
                    expected_digest: None,
                    expected_size: None,
                }
            }
        };
        ensure!(valid_id(&row.id), "Повреждена запись загрузки.");
        if matches!(row.status, Status::Running | Status::Publishing) {
            if row.status == Status::Publishing && row.published() {
                row.status = Status::Done;
            } else {
                row.status = Status::Failed;
                row.error = Some("Загрузка была прервана. Нажмите Повторить.".into());
            }
            save(data, &row)?;
        }
        result.push(row);
    }
    result.sort_by_key(|row| std::cmp::Reverse(row.created));
    Ok(result)
}

pub(crate) fn command(_data: &Path) -> Result<Command> {
    let mut command = Command::new(crate::components::tool("yt-dlp")?);
    command.args([
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-playlist",
        "--color",
        "never",
        "--socket-timeout",
        "20",
        "--retries",
        "2",
        "--fragment-retries",
        "2",
    ]);
    let deno = crate::components::tool("deno")?;
    if deno.is_file() {
        command
            .arg("--js-runtimes")
            .arg(format!("deno:{}", deno.display()));
    }
    Ok(command)
}
fn failed(stderr: &[u8]) -> anyhow::Error {
    let output = String::from_utf8_lossy(stderr);
    // Classify the actual error, not a warning or an authentication hint elsewhere
    // in stderr. YouTube's bot check also contains the words "sign in".
    let detail = output.lines().rev().map(str::trim)
        .find(|line| line.starts_with("ERROR:"))
        .or_else(|| output.lines().rev().map(str::trim).find(|line| !line.is_empty()))
        .unwrap_or("");
    let message = detail.to_lowercase();
    let explanation = if message.contains("not a bot") || message.contains("captcha") {
        "Сайт отклонил запрос yt-dlp при проверке на бота. Это не означает, что запись закрыта. Обновите компоненты загрузки и повторите."
    } else if message.contains("429") || message.contains("too many requests") {
        "Сервис ограничил запросы. Повторите загрузку позже."
    } else if message.contains("private video") || message.contains("video is private")
        || message.contains("members-only") || message.contains("members only")
    {
        "Источник ограничил доступ к записи. Проверьте, доступна ли она вам на самом сайте."
    } else if message.contains("confirm your age") || message.contains("age-restricted") {
        "Для этой записи сайт требует подтверждения возраста. Вход в музыкальные сервисы плеера не передает авторизацию в yt-dlp."
    } else if message.contains("sign in") || message.contains("login required") {
        "Сайт запросил авторизацию для yt-dlp. Подключение музыкального аккаунта в плеере не авторизует загрузчик."
    } else if message.contains("unsupported url") {
        "Этот источник пока не поддерживается компонентом загрузки."
    } else if message.contains("javascript") || message.contains("challenge") || message.contains("ejs") {
        "Загрузчик не смог обработать страницу. Обновите компоненты загрузки и повторите действие."
    } else {
        "Не удалось получить запись. Проверьте ссылку или обновите компоненты загрузки."
    };
    // Keep the source error in download history, but not a traceback or the long
    // generic cookie instructions. The first line remains the user-facing summary.
    let detail = detail.split(". Use --cookies").next().unwrap_or(detail)
        .chars().filter(|c| !c.is_control()).take(800).collect::<String>();
    if detail.is_empty() { anyhow::anyhow!(explanation) }
    else { anyhow::anyhow!("{explanation}\n{detail}") }
}

pub fn search(data: &Path, query: &str, cancel: &AtomicBool) -> Result<Vec<SearchHit>> {
    let query = query.trim();
    ensure!(!query.is_empty(), "Введите название или исполнителя.");
    let mut command = command(data)?;
    command
        .args([
            "--flat-playlist",
            "--dump-single-json",
            "--skip-download",
            "--",
        ])
        .arg(format!("ytsearch20:{query}"));
    let output = process::run(
        command,
        None,
        cancel,
        Duration::from_secs(120),
        4 * 1024 * 1024,
    )?;
    if !output.success {
        return Err(failed(&output.stderr));
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Компонент вернул неизвестный ответ поиска.")?;
    let mut result = Vec::new();
    for row in value
        .get("entries")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .take(30)
    {
        let Some(url) = row
            .get("webpage_url")
            .or_else(|| row.get("url"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        if network::public_url(url).is_err() {
            continue;
        }
        result.push(SearchHit {
            title: row
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Audio")
                .into(),
            artist: row
                .get("uploader")
                .or_else(|| row.get("channel"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            url: url.into(),
            duration: row.get("duration").and_then(|v| v.as_f64()),
        });
    }
    Ok(result)
}

fn safe_name(value: &str) -> String {
    let name = value
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect::<String>();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() {
        "Audio".into()
    } else {
        name.into()
    }
}

async fn cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn receive(url: &str, path: &Path, cancel: &AtomicBool, progress: &AtomicU64) -> Result<()> {
    let url = network::public_url(url)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client=reqwest::Client::builder().user_agent("Almavorn").connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(3600)).build()?;
        let mut response=tokio::select! {
            response=client.get(url).send()=>response.context("Не удалось начать загрузку.")?,
            _=cancelled(cancel)=>bail!("Загрузка отменена."),
        };
        ensure!(response.status().is_success(),"Сервис отклонил загрузку (HTTP {}).",response.status().as_u16());
        ensure!(response.content_length().is_none_or(|n|n<=1024*1024*1024),"Запись превышает предел загрузки в 1 ГБ.");
        let mut file=File::options().write(true).create_new(true).open(path)?;
        let mut total=0u64;
        loop {
            let chunk=tokio::select! {
                chunk=tokio::time::timeout(Duration::from_secs(45),response.chunk())=>chunk.context("Сервис перестал передавать файл. Повторите загрузку.")?.context("Соединение прервано во время загрузки.")?,
                _=cancelled(cancel)=>bail!("Загрузка отменена."),
            };
            let Some(chunk)=chunk else{break;};
            total+=chunk.len() as u64;ensure!(total<=1024*1024*1024,"Запись превышает предел загрузки в 1 ГБ.");
            file.write_all(&chunk)?;progress.store(total,Ordering::Relaxed);
        }
        file.sync_all()?;Ok(())
    })
}

fn is_youtube_url(value: &str) -> bool {
    network::public_url(value).ok().and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "youtu.be" || host == "youtube.com" || host.ends_with(".youtube.com"))
}

fn youtube_cover(path: &Path, video_id: &str, cancel: &AtomicBool) -> Result<()> {
    ensure!(video_id.len() == 11 && video_id.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
        "Неизвестный идентификатор видео.");
    let mut document = crate::tags::Document::read(path)?;
    if document.blocks.iter().any(|block| !block.covers.is_empty()) {
        return Ok(());
    }
    let block = document.blocks.iter_mut().find(|block| block.writable)
        .context("Формат не поддерживает запись обложки.")?;
    ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена.");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client.get(format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg"))
        .send()?.error_for_status()?;
    const MAX_COVER: u64 = 8 * 1024 * 1024;
    ensure!(response.content_length().is_none_or(|size| size <= MAX_COVER), "Обложка слишком большая.");
    let mut bytes = Vec::new();
    response.take(MAX_COVER + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_COVER, "Обложка слишком большая.");
    ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена.");
    crate::artwork::validate_bytes(&bytes)?;
    let mut picture = lofty::picture::Picture::from_reader(&mut std::io::Cursor::new(bytes))?;
    picture.set_pic_type(lofty::picture::PictureType::CoverFront);
    block.covers.push(crate::tags::Cover { picture, remove: false });
    document.prepare()?.replace()?.accept()?;
    Ok(())
}

fn cobalt(
    data: &Path,
    config: &Config,
    url: &str,
    cancel: &AtomicBool,
    progress: &AtomicU64,
    directory: &Path,
) -> Result<PathBuf> {
    ensure!(
        !config.cobalt_url.is_empty(),
        "Укажите адрес Cobalt в настройках загрузок."
    );
    let endpoint = cobalt_endpoint(&config.cobalt_url)?;
    let account = cobalt_key(&endpoint);
    let mut request=network::client()?.post(endpoint).header("Accept","application/json").json(&serde_json::json!({"url":url,"downloadMode":"audio","audioFormat":"mp3","localProcessing":"disabled"}));
    if let Some(key) = secrets::get(data, &account)? {
        request = request.header("Authorization", format!("Api-Key {key}"));
    }
    let value: serde_json::Value =
        network::json(request.send().context("Не удалось связаться с Cobalt.")?)?;
    match value.get("status").and_then(|v| v.as_str()) {
        Some("tunnel" | "redirect") => {}
        Some("picker") => bail!("В ссылке несколько записей. Укажите ссылку на одну композицию."),
        Some("local-processing") => {
            bail!("Этот сервер Cobalt требует дополнительной обработки. Выберите yt-dlp.")
        }
        _ => bail!("Cobalt не смог получить запись. Проверьте ссылку и доступ к серверу."),
    }
    let url = value
        .get("url")
        .and_then(|v| v.as_str())
        .context("Cobalt не вернул ссылку на аудио.")?;
    let file = directory.join("audio.mp3");
    receive(url, &file, cancel, progress)?;
    Ok(file)
}

pub fn download(
    data: &Path,
    config: &Config,
    mut job: Download,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Download {
    let result = (|| -> Result<()> {
        let requested = job.folder.clone();
        let (folder, staging) = staging_folder(&requested, cancel)?;
        job.notice = if requested.canonicalize().ok().as_ref() != Some(&folder) {
            Some(format!(
                "Выбранная папка недоступна. Папка для этой загрузки: {}.",
                folder.display()
            ))
        } else {
            None
        };
        job.folder = folder;
        save(data, &job)?;
        let file = match job.engine {
            Engine::Cobalt => cobalt(data, config, &job.url, cancel, progress, staging.path())?,
            Engine::YtDlp => {
                let mut command = command(data)?;
                command.args(["--no-progress","--no-simulate","--max-filesize","1G","--format","bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio[ext=flac]/bestaudio[ext=wav]/bestaudio[acodec=vorbis]/bestaudio","--print","after_move:%(filepath)j","--print","after_move:almavorn-video-id:%(id)j","--output"])
                    .arg(staging.path().join("%(title).100B [%(id)s].%(ext)s")).arg("--").arg(&job.url);
                let output = process::run(
                    command,
                    None,
                    cancel,
                    Duration::from_secs(3600),
                    1024 * 1024,
                )?;
                if !output.success {
                    return Err(failed(&output.stderr));
                }
                let stdout = String::from_utf8(output.stdout)?;
                let path = stdout
                    .lines()
                    .rev()
                    .find_map(|line| serde_json::from_str::<String>(line).ok())
                    .context("Компонент не сообщил путь скачанного файла.")?;
                let path = PathBuf::from(path).canonicalize()?;
                ensure!(
                    path.starts_with(staging.path().canonicalize()?) && path.is_file(),
                    "Компонент вернул неверный путь файла."
                );
                if is_youtube_url(&job.url) {
                    let id = stdout.lines()
                        .find_map(|line| line.strip_prefix("almavorn-video-id:")
                            .and_then(|value| serde_json::from_str::<String>(value).ok()));
                    let added = id.as_deref().context("Видео не вернуло идентификатор.")
                        .and_then(|id| youtube_cover(&path, id, cancel));
                    if added.is_err() && !cancel.load(Ordering::Relaxed) {
                        let note = "Не удалось добавить обложку видео. Аудиофайл сохранен.";
                        job.notice = Some(match job.notice.take() {
                            Some(previous) => format!("{previous}\n{note}"),
                            None => note.into(),
                        });
                    }
                }
                path
            }
        };
        ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена.");
        crate::media::read_track(&file)
            .context("Скачанная запись не является поддерживаемым аудиофайлом.")?;
        crate::audio::Audio::prepare(&file)
            .context("Формат записи не воспроизводится. Попробуйте Cobalt с форматом MP3.")?;
        let base = if job.name.trim().is_empty() {
            file.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            job.name.clone()
        };
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
        let destination = job
            .folder
            .join(format!("{}.{}", safe_name(&base), extension));
        ensure!(
            !destination.try_exists()?,
            "Такой файл уже есть. Измените имя или папку загрузки."
        );
        job.expected_digest = Some(network::file_digest(&file)?);
        job.expected_size = Some(std::fs::metadata(&file)?.len());
        ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена.");
        job.path = Some(destination.clone());
        job.status = Status::Publishing;
        save(data, &job)?;
        File::open(&file)?.sync_all()?;
        crate::tags::move_noreplace(&file, &destination)?;
        progress.store(std::fs::metadata(&destination)?.len(), Ordering::Relaxed);
        job.status = Status::Done;
        job.error = None;
        Ok(())
    })();
    if let Err(error) = result {
        job.status = if job.status == Status::Publishing && job.published() {
            Status::Done
        } else if cancel.load(Ordering::Relaxed) {
            Status::Cancelled
        } else {
            Status::Failed
        };
        job.error = Some(format!("{error:#}"));
    }
    if let Err(error) = save(data, &job) {
        job.error = Some(format!("Не удалось сохранить запись загрузки: {error:#}"));
    }
    job
}

static TOOLS_FAILURE: Mutex<Option<(Instant, String)>> = Mutex::new(None);

fn tools_ready(cancel: &AtomicBool) -> Result<bool> {
    for name in ["yt-dlp", "deno"] {
        let mut command = Command::new(crate::components::tool(name)?);
        command.arg("--version");
        let output = process::run(command, None, cancel, Duration::from_secs(15), 16 * 1024);
        ensure!(!cancel.load(Ordering::Relaxed), "Подготовка загрузки отменена.");
        match output {
            Ok(output) if output.success => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub fn prepare_tools(data: &Path, cancel: &AtomicBool, progress: &AtomicU64) -> Result<()> {
    prepare_tools_inner(data, cancel, progress, false)
}

pub fn install_tools(data: &Path, cancel: &AtomicBool, progress: &AtomicU64) -> Result<()> {
    prepare_tools_inner(data, cancel, progress, true)
}

fn prepare_tools_inner(
    _data: &Path,
    cancel: &AtomicBool,
    progress: &AtomicU64,
    update: bool,
) -> Result<()> {
    // Serialize installation and avoid repeated network attempts for a queued batch.
    let mut failure = loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Подготовка загрузки отменена.");
        match TOOLS_FAILURE.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::Poisoned(error)) => break error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    if !update {
        if tools_ready(cancel)? {
            *failure = None;
            return Ok(());
        }
        if let Some((time, message)) = failure.as_ref()
            && time.elapsed() < Duration::from_secs(60)
        {
            bail!("{message}");
        }
    }
    let result = fetch_tools(cancel, progress)
        .and_then(|()| {
            ensure!(tools_ready(cancel)?, "Установленные компоненты не запускаются на этой системе.");
            Ok(())
        })
        .context("Не удалось подготовить скачивание. Проверьте подключение к интернету и нажмите Установить / обновить компоненты");
    if !cancel.load(Ordering::Relaxed) {
        *failure = result.as_ref().err()
            .map(|error| (Instant::now(), format!("{error:#}")));
    }
    progress.store(0, Ordering::Relaxed);
    result
}

fn fetch_tools(cancel: &AtomicBool, progress: &AtomicU64) -> Result<()> {
    let architecture = std::env::consts::ARCH;
    let (yt, deno) = match architecture {
        "x86_64" => ("yt-dlp_linux", "deno-x86_64-unknown-linux-gnu.zip"),
        "aarch64" => ("yt-dlp_linux_aarch64", "deno-aarch64-unknown-linux-gnu.zip"),
        _ => bail!("Для этой архитектуры установите yt-dlp и Deno через менеджер пакетов."),
    };
    let tools = crate::components::directory()?;
    std::fs::create_dir_all(&tools)?;
    crate::components::validate(&tools)?;
    let temporary = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&tools)?;
    for (repository, asset, filename) in [
        ("yt-dlp/yt-dlp", yt, "yt-dlp"),
        ("denoland/deno", deno, "deno.zip"),
    ] {
        let release: serde_json::Value = network::json(
            network::client()?
                .get(format!(
                    "https://api.github.com/repos/{repository}/releases/latest"
                ))
                .send()
                .context("Не удалось получить список компонентов.")?,
        )?;
        let asset = release["assets"]
            .as_array()
            .and_then(|items| items.iter().find(|a| a["name"].as_str() == Some(asset)))
            .context("Компонент для этой системы не найден.")?;
        let url = asset["browser_download_url"]
            .as_str()
            .context("Не найдена ссылка компонента.")?;
        ensure!(
            url.starts_with(&format!(
                "https://github.com/{repository}/releases/download/"
            )),
            "Получена неверная ссылка компонента."
        );
        let digest = asset["digest"]
            .as_str()
            .and_then(|s| s.strip_prefix("sha256:"))
            .context("Не удалось проверить происхождение компонента. Повторите позже.")?;
        let destination = temporary.path().join(filename);
        receive(url, &destination, cancel, progress)?;
        ensure!(
            network::file_digest(&destination)? == digest,
            "Файл компонента поврежден. Повторите установку."
        );
    }
    let mut zip = zip::ZipArchive::new(File::open(temporary.path().join("deno.zip"))?)?;
    let mut entry = zip.by_name("deno")?;
    ensure!(
        entry.size() < 200 * 1024 * 1024,
        "Архив Deno слишком большой."
    );
    let mut file = File::create(temporary.path().join("deno"))?;
    std::io::copy(&mut entry.by_ref().take(200 * 1024 * 1024 + 1), &mut file)?;
    file.sync_all()?;
    for name in ["yt-dlp", "deno"] {
        ensure!(!cancel.load(Ordering::Relaxed), "Установка отменена.");
        let source = temporary.path().join(name);
        let mut magic = [0u8; 4];
        File::open(&source)?.read_exact(&mut magic)?;
        ensure!(
            &magic == b"\x7fELF",
            "Получен неверный файл компонента. Повторите установку."
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::rename(source, tools.join(name))?;
    }
    File::open(&tools)?.sync_all()?;
    Ok(())
}
