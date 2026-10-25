use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::{Client, Response};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{Read, Write},
    path::Path,
    time::Duration,
};

#[derive(Debug)]
pub(crate) struct ConnectionFailure;
impl std::fmt::Display for ConnectionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Нет соединения с сервисом.")
    }
}
impl std::error::Error for ConnectionFailure {}
pub(crate) fn connection_error(_: reqwest::Error) -> anyhow::Error {
    ConnectionFailure.into()
}
pub(crate) fn is_connection_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ConnectionFailure>().is_some()
}

pub(crate) fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!(
            "Almavorn/",
            env!("CARGO_PKG_VERSION"),
            " (https://github.com/Molten-Zharr/Almavorn)"
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("Не удалось подготовить подключение к сервису.")
}

pub(crate) fn public_url(value: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(value).context("Введите полный адрес, начиная с https://.")?;
    ensure!(
        matches!(url.scheme(), "https" | "http")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none(),
        "Нужна ссылка HTTP или HTTPS без пароля в адресе."
    );
    Ok(url)
}

pub(crate) fn json<T: DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        bail!(
            "{} (HTTP {}).",
            match status.as_u16() {
                401 => "Нужно войти в сервис заново",
                403 => "Сервис не разрешил доступ к этой записи",
                404 => "Запись не найдена",
                429 => "Слишком много запросов. Повторите поиск позже",
                _ => "Сервис временно не ответил",
            },
            status.as_u16()
        );
    }
    let mut bytes = Vec::new();
    response
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ConnectionFailure)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Ответ сервиса слишком большой."
    );
    serde_json::from_slice(&bytes).context("Сервис вернул ответ в неизвестном формате.")
}

pub(crate) fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("Не найдена папка сохранения.")?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(file.as_file_mut(), value)?;
    file.as_file_mut().write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub(crate) fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= 4 * 1024 * 1024,
        "Сохраненный файл слишком большой."
    );
    serde_json::from_reader(file).context("Не удалось прочитать сохраненный файл.")
}

pub(crate) fn file_digest(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
