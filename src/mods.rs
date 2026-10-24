use crate::network;
use anyhow::{Context, Result, ensure};
use rhai::{Dynamic, Engine, Scope};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub api: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default = "empty_object")]
    pub settings: Value,
    #[serde(default)]
    pub setting_labels: std::collections::BTreeMap<String, String>,
}
fn empty_object() -> Value {
    json!({})
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Mod {
    pub manifest: Manifest,
    pub script: String,
    pub enabled: bool,
    #[serde(default = "empty_object")]
    pub settings: Value,
    #[serde(default = "empty_object")]
    pub state: Value,
}
#[derive(Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct Output {
    pub message: String,
    pub panel: String,
    pub commands: Vec<Command>,
    pub state: Option<Value>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Command {
    PlayPause,
    Stop,
    Next,
    Previous,
    Volume { value: f32 },
    Seek { milliseconds: u64 },
    PlayTrack { playlist: i64, entry: i64 },
    Palette { id: String },
    Open { page: String },
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !id.starts_with('.')
        && !id.ends_with('.')
}
fn path(data: &Path, id: &str) -> Result<PathBuf> {
    ensure!(valid_id(id), "Недопустимое имя мода.");
    Ok(data.join("mods").join(format!("{id}.json")))
}
impl Mod {
    pub fn permission(&self, name: &str) -> bool {
        self.manifest.permissions.iter().any(|p| p == name)
    }
    pub fn new(name: String) -> Self {
        let manifest = Manifest {
            api: 1,
            id: format!("local-{:016x}", fastrand::u64(..)),
            name,
            version: "1.0.0".into(),
            description: "Показывает текущую композицию и переключает следующую.".into(),
            author: String::new(),
            permissions: vec!["playback".into()],
            hosts: vec![],
            actions: vec![
                Action {
                    id: "show".into(),
                    name: "Текущая композиция".into(),
                },
                Action {
                    id: "next".into(),
                    name: "Следующая композиция".into(),
                },
            ],
            events: vec!["track_changed".into()],
            settings: json!({"prefix":"Сейчас играет"}),
            setting_labels: std::collections::BTreeMap::from([(
                "prefix".into(),
                "Подпись перед названием".into(),
            )]),
        };
        Self {
            settings: manifest.settings.clone(),
            manifest,
            script: include_str!("mods/template.rhai").into(),
            enabled: true,
            state: json!({}),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.manifest.api == 1,
            "Версия API мода не поддерживается этим плеером."
        );
        ensure!(valid_id(&self.manifest.id), "Недопустимое имя мода.");
        ensure!(
            !self.manifest.name.trim().is_empty() && self.manifest.name.chars().count() <= 100,
            "Введите название мода до 100 символов."
        );
        ensure!(
            self.script.len() <= 256 * 1024 && self.manifest.description.len() <= 8192,
            "Мод слишком большой."
        );
        ensure!(
            self.settings.is_object() && self.manifest.settings.is_object(),
            "Настройки мода должны быть объектом JSON."
        );
        ensure!(
            self.manifest.actions.len() <= 32
                && self.manifest.events.len() <= 3
                && self.manifest.hosts.len() <= 8,
            "В моде слишком много действий или источников."
        );
        let mut ids = BTreeSet::new();
        for action in &self.manifest.actions {
            ensure!(
                valid_id(&action.id)
                    && !action.name.trim().is_empty()
                    && action.name.chars().count() <= 80
                    && ids.insert(&action.id),
                "Проверьте названия и уникальность действий мода."
            );
        }
        for event in &self.manifest.events {
            ensure!(
                matches!(event.as_str(), "startup" | "track_changed" | "tick"),
                "Неизвестное событие мода: {event}"
            );
        }
        for permission in &self.manifest.permissions {
            ensure!(
                matches!(
                    permission.as_str(),
                    "playback" | "library_read" | "appearance" | "network"
                ),
                "Неизвестное разрешение мода: {permission}"
            );
        }
        for host in &self.manifest.hosts {
            ensure!(
                host.len() <= 253
                    && host.contains('.')
                    && host
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
                    && host.parse::<IpAddr>().is_err(),
                "Укажите доменное имя источника мода."
            );
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 1024 * 1024,
            "Данные мода превышают 1 МБ."
        );
        let engine = engine(self, Arc::new(AtomicBool::new(false)));
        engine
            .compile(&self.script)
            .map_err(|error| anyhow::anyhow!("Ошибка кода мода: {error}"))?;
        Ok(())
    }
}
pub fn list(data: &Path) -> Result<Vec<Mod>> {
    let root = data.join("mods");
    if !root.try_exists()? {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for entry in std::fs::read_dir(root)? {
        let file = entry?.path();
        if file.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let id = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !valid_id(id) {
            continue;
        }
        let mut value = match network::load_json::<Mod>(&file) {
            Ok(value) => value,
            Err(_) => {
                let mut value = Mod::new(format!("Поврежденный мод: {id}"));
                value.manifest.id = id.into();
                value.manifest.description =
                    "Не удалось прочитать мод. Установите его архив заново или удалите запись."
                        .into();
                value.enabled = false;
                value
            }
        };
        if value.manifest.id != id {
            value.manifest.id = id.into();
            value.enabled = false;
            value.manifest.description =
                "Описание мода повреждено. Установите архив заново.".into();
        }
        if let Err(error) = value.validate() {
            value.enabled = false;
            value.manifest.description = format!("{error:#}");
        }
        result.push(value);
    }
    result.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    let mut active = 0;
    for value in &mut result {
        if value.enabled {
            active += 1;
            if active > 64 {
                value.enabled = false;
                value.manifest.description =
                    "Достигнут предел: 64 включенных мода. Отключите другой мод.".into();
            }
        }
    }
    Ok(result)
}
pub fn save(data: &Path, value: &Mod) -> Result<()> {
    value.validate()?;
    if value.enabled {
        ensure!(
            list(data)?
                .iter()
                .filter(|m| m.enabled && m.manifest.id != value.manifest.id)
                .count()
                < 64,
            "Одновременно доступны 64 мода. Отключите другой мод."
        );
    }
    network::save_json(&path(data, &value.manifest.id)?, value)
}
pub fn remove(data: &Path, id: &str) -> Result<()> {
    std::fs::remove_file(path(data, id)?)?;
    Ok(())
}
pub fn read_package(source: &Path) -> Result<Mod> {
    fn text(mut source: impl Read) -> Result<String> {
        let mut text = String::new();
        source
            .by_ref()
            .take(256 * 1024 + 1)
            .read_to_string(&mut text)?;
        ensure!(text.len() <= 256 * 1024, "Файл мода превышает 256 КБ.");
        Ok(text)
    }
    let (manifest, script) = if source.is_dir() {
        (
            text(File::open(source.join("manifest.json"))?)?,
            text(File::open(source.join("main.rhai"))?)?,
        )
    } else {
        ensure!(
            std::fs::metadata(source)?.len() <= 4 * 1024 * 1024,
            "Архив мода превышает 4 МБ."
        );
        let mut archive = zip::ZipArchive::new(File::open(source)?)?;
        ensure!(archive.len() <= 64, "В архиве слишком много файлов.");
        let manifest = text(
            archive
                .by_name("manifest.json")
                .context("В корне архива нужен manifest.json.")?,
        )?;
        let script = text(
            archive
                .by_name("main.rhai")
                .context("В корне архива нужен main.rhai.")?,
        )?;
        (manifest, script)
    };
    let manifest: Manifest =
        serde_json::from_str(&manifest).context("Не удалось прочитать описание мода.")?;
    let value = Mod {
        settings: manifest.settings.clone(),
        manifest,
        script,
        enabled: true,
        state: empty_object(),
    };
    value.validate()?;
    Ok(value)
}
pub fn install(data: &Path, mut value: Mod) -> Result<Mod> {
    let target = path(data, &value.manifest.id)?;
    if target.try_exists()?
        && let Ok(old) = network::load_json::<Mod>(&target)
    {
        value.enabled = old.enabled;
        value.state = old.state;
        if let (Some(defaults), Some(previous)) =
            (value.settings.as_object_mut(), old.settings.as_object())
        {
            for (k, v) in previous {
                defaults.insert(k.clone(), v.clone());
            }
        }
    }
    save(data, &value)?;
    Ok(value)
}
pub fn export(value: &Mod, destination: &Path) -> Result<()> {
    use zip::write::SimpleFileOptions;
    let parent = destination.parent().context("Укажите папку архива.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut zip = zip::ZipWriter::new(file.as_file_mut());
        zip.start_file("manifest.json", SimpleFileOptions::default())?;
        zip.write_all(&serde_json::to_vec_pretty(&value.manifest)?)?;
        zip.start_file("main.rhai", SimpleFileOptions::default())?;
        zip.write_all(value.script.as_bytes())?;
        zip.finish()?;
    }
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist_noclobber(destination)
        .map_err(|error| error.error)
        .context("Архив с таким именем уже есть или папка недоступна.")?;
    Ok(())
}
fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && ip.octets()[0] != 0
                && ip.octets()[0] < 224
                && !(ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
                && !(ip.octets()[0] == 198 && matches!(ip.octets()[1], 18 | 19))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let segment = ip.segments()[0];
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && segment & 0xfe00 != 0xfc00
                && segment & 0xffc0 != 0xfe80
                && !(segment == 0x2001 && ip.segments()[1] == 0xdb8)
        }
    }
}
fn engine(value: &Mod, cancel: Arc<AtomicBool>) -> Engine {
    let mut engine = Engine::new();
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.disable_symbol("eval");
    engine.disable_symbol("import");
    engine
        .set_max_operations(200_000)
        .set_max_call_levels(32)
        .set_max_expr_depths(32, 32)
        .set_max_string_size(256 * 1024)
        .set_max_array_size(4096)
        .set_max_map_size(1024)
        .set_max_variables(256)
        .set_max_functions(128);
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    let started = Instant::now();
    let network_cancel = cancel.clone();
    engine.on_progress(move |_| {
        if cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(30) {
            Some(Dynamic::from("Мод остановлен: превышено время выполнения."))
        } else {
            None
        }
    });
    if value.permission("network") {
        let hosts = value.manifest.hosts.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        engine.register_fn(
            "http_json",
            move |address: &str| -> std::result::Result<Dynamic, Box<rhai::EvalAltResult>> {
                let result = (|| -> Result<Dynamic> {
                    ensure!(
                        calls.fetch_add(1, Ordering::Relaxed) < 4,
                        "Мод выполнил слишком много сетевых запросов."
                    );
                    let url = network::public_url(address)?;
                    ensure!(
                        url.scheme() == "https" && url.port_or_known_default() == Some(443),
                        "Моду разрешены только HTTPS-запросы."
                    );
                    let host = url.host_str().context("Не указан источник.")?;
                    ensure!(
                        hosts.iter().any(|h| h.eq_ignore_ascii_case(host)),
                        "Источник не указан в разрешениях мода."
                    );
                    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().max_blocking_threads(1).build()?;
                    let value=runtime.block_on(async {
                        tokio::select!{
                            result=async {
                                let addresses=tokio::net::lookup_host((host,443)).await?.collect::<Vec<_>>();ensure!(!addresses.is_empty()&&addresses.iter().all(|a|public_ip(a.ip())),"Мод не может обращаться к локальной сети.");
                                let client=reqwest::Client::builder().user_agent("Almavorn-mod/1").timeout(Duration::from_secs(8)).connect_timeout(Duration::from_secs(5)).redirect(reqwest::redirect::Policy::none()).no_proxy().resolve_to_addrs(host,&addresses).build()?;
                                let mut response=client.get(url.clone()).send().await.map_err(|_|anyhow::anyhow!("Источник мода не отвечает."))?;ensure!(response.status().is_success(),"Источник отклонил запрос мода.");let mut bytes=vec![];
                                while let Some(chunk)=response.chunk().await.map_err(|_|anyhow::anyhow!("Ответ источника прерван."))?{ensure!(bytes.len()+chunk.len()<=256*1024,"Ответ источника слишком большой.");bytes.extend_from_slice(&chunk);}
                                serde_json::from_slice::<Value>(&bytes).context("Источник вернул неверный JSON.")
                            }=>result,
                            _=tokio::time::sleep(Duration::from_secs(8).min(Duration::from_secs(30).saturating_sub(started.elapsed())))=>Err(anyhow::anyhow!("Источник мода не ответил вовремя.")),
                            _=async{while !network_cancel.load(Ordering::Relaxed){tokio::time::sleep(Duration::from_millis(50)).await;}}=>Err(anyhow::anyhow!("Выполнение мода отменено.")),
                        }
                    });runtime.shutdown_background();let value=value?;
                    rhai::serde::to_dynamic(value).map_err(|e| anyhow::anyhow!("{e}"))
                })();
                result.map_err(|error| format!("{error:#}").into())
            },
        );
    }
    engine
}
pub fn execute(
    value: &Mod,
    event: &str,
    context: Value,
    cancel: Arc<AtomicBool>,
) -> Result<Output> {
    ensure!(value.enabled, "Мод отключен.");
    value.validate()?;
    let engine = engine(value, cancel);
    let ast = engine
        .compile(&value.script)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let context = rhai::serde::to_dynamic(context)?;
    let settings = rhai::serde::to_dynamic(&value.settings)?;
    let state = rhai::serde::to_dynamic(&value.state)?;
    let result: Dynamic = engine
        .call_fn(
            &mut Scope::new(),
            &ast,
            "handle",
            (event.to_owned(), context, settings, state),
        )
        .map_err(|error| anyhow::anyhow!("{}: {error}", value.manifest.name))?;
    let output: Output =
        rhai::serde::from_dynamic(&result).context("Мод вернул неизвестный результат.")?;
    ensure!(
        output.commands.len() <= 16
            && output.message.len() <= 4096
            && output.panel.len() <= 64 * 1024,
        "Мод вернул слишком большой результат."
    );
    for command in &output.commands {
        match command {
            Command::Palette { .. } => ensure!(
                value.permission("appearance"),
                "Моду не разрешено изменять оформление."
            ),
            Command::Open { page } => ensure!(
                event.starts_with("action:")
                    && matches!(page.as_str(), "lyrics" | "downloads" | "services" | "mods"),
                "Мод запросил неизвестное окно."
            ),
            Command::PlayTrack { .. } => ensure!(
                value.permission("playback") && value.permission("library_read"),
                "Моду не разрешено выбирать композиции библиотеки."
            ),
            Command::Volume { value: volume } => ensure!(
                value.permission("playback") && volume.is_finite() && (0.0..=1.0).contains(volume),
                "Мод передал недопустимую громкость."
            ),
            _ => ensure!(
                value.permission("playback"),
                "Моду не разрешено управлять воспроизведением."
            ),
        }
    }
    if let Some(state) = &output.state {
        ensure!(
            serde_json::to_vec(state)?.len() <= 256 * 1024,
            "Состояние мода превышает 256 КБ."
        );
    }
    Ok(output)
}

pub(crate) fn persist_runtime(data: &Path, value: &Mod) -> Result<()> {
    ensure!(
        serde_json::to_vec(value)?.len() <= 1024 * 1024,
        "Данные мода превышают 1 МБ."
    );
    network::save_json(&path(data, &value.manifest.id)?, value)
}
pub(crate) fn save_configuration(data: &Path, mut value: Mod) -> Result<Mod> {
    let file = path(data, &value.manifest.id)?;
    if let Ok(old) = network::load_json::<Mod>(&file) {
        value.state = old.state;
    }
    save(data, &value)?;
    Ok(value)
}
