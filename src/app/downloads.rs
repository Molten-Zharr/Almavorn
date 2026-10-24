use super::{
    App, Dialog, FormField, FormPurpose,
    background::{Background, LatestJob, background, poll},
};
use crate::{
    downloads::{self, Config, Download, Engine, SearchHit, Status},
    input::{Key, KeyPress},
    model::PlaylistKind,
};
use anyhow::{Context, Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone)]
enum Auxiliary {
    Load,
    Search(String),
    Configure(Config, Option<String>),
    Tools,
}
#[derive(Clone)]
struct Request {
    operation: Auxiliary,
    cancel: Arc<AtomicBool>,
}
enum Response {
    Loaded(Config, Vec<Download>),
    Found(Vec<SearchHit>),
    Configured(Config),
    Installed,
}
struct Job {
    id: String,
    receiver: Background<Download>,
    cancel: Arc<AtomicBool>,
}

pub struct DownloadsState {
    pub config: Config,
    pub rows: Vec<Download>,
    pub results: Vec<SearchHit>,
    pub query: String,
    pub error: Option<String>,
    pub progress: Arc<AtomicU64>,
    pub active: Option<String>,
    pub initialized: bool,
    job: Option<Job>,
    aux: LatestJob<Request, Response>,
    cancel: Option<Arc<AtomicBool>>,
    pending_import: std::collections::VecDeque<String>,
    importing: Option<String>,
}
impl Default for DownloadsState {
    fn default() -> Self {
        Self {
            config: Config::default(),
            rows: vec![],
            results: vec![],
            query: String::new(),
            error: None,
            progress: Arc::new(AtomicU64::new(0)),
            active: None,
            initialized: false,
            job: None,
            aux: LatestJob::default(),
            cancel: None,
            pending_import: Default::default(),
            importing: None,
        }
    }
}
impl DownloadsState {
    pub(super) fn cancel_all(&self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for DownloadsState {
    fn drop(&mut self) {
        self.cancel_all();
    }
}

#[derive(Clone, Default)]
pub struct DownloadsView {
    pub search: bool,
    pub selected: usize,
    pub offset: usize,
    pub remove: Option<String>,
}
#[derive(Clone, Copy)]
pub enum DownloadAction {
    Row(usize),
    Open,
    New,
    Search,
    Queue,
    Settings,
    Engine,
    Tools,
    Cancel,
    Retry,
    Edit,
    Remove,
    ConfirmRemove,
    Import,
    History,
    Back,
}

impl App {
    pub(super) fn initialize_downloads(&mut self) {
        self.downloads_request(Auxiliary::Load);
    }
    pub(super) fn open_downloads(&mut self) {
        self.view.dialog = Some(Dialog::Downloads(DownloadsView::default()));
        if !self.downloads.initialized && !self.downloads_loading() {
            self.initialize_downloads();
        }
    }
    pub fn downloads_loading(&self) -> bool {
        self.downloads.aux.requested().is_some()
    }
    fn downloads_request(&mut self, operation: Auxiliary) {
        if let Some(cancel) = self.downloads.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.downloads.cancel = Some(cancel.clone());
        self.downloads.error = None;
        self.downloads.aux.request(Request { operation, cancel });
        self.start_download_aux();
    }
    fn start_download_aux(&mut self) {
        let data = self.data_directory();
        let progress = self.downloads.progress.clone();
        let runtime = self.runtime.as_ref().expect("runtime");
        self.downloads.aux.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                ensure!(
                    !request.cancel.load(Ordering::Relaxed),
                    "Операция отменена."
                );
                match request.operation {
                    Auxiliary::Load => Ok(Response::Loaded(
                        downloads::config(&data)?,
                        downloads::list(&data)?,
                    )),
                    Auxiliary::Search(query) => Ok(Response::Found(downloads::search(
                        &data,
                        &query,
                        &request.cancel,
                    )?)),
                    Auxiliary::Configure(config, key) => {
                        downloads::save_config(&data, &config, key.as_deref())?;
                        Ok(Response::Configured(config))
                    }
                    Auxiliary::Tools => {
                        downloads::install_tools(&data, &request.cancel, &progress)?;
                        Ok(Response::Installed)
                    }
                }
            })
        });
    }
    pub(super) fn configure_downloads(&mut self, config: Config, key: Option<String>) {
        self.view.dialog = Some(Dialog::Downloads(DownloadsView::default()));
        self.downloads_request(Auxiliary::Configure(config, key));
    }
    pub(super) fn search_downloads(&mut self, query: String) {
        self.downloads.query = query.clone();
        self.downloads.results.clear();
        self.view.dialog = Some(Dialog::Downloads(DownloadsView {
            search: true,
            ..Default::default()
        }));
        self.downloads_request(Auxiliary::Search(query));
    }
    pub(super) fn queue_download(
        &mut self,
        mut job: Download,
        replacing: Option<&str>,
    ) -> Result<()> {
        ensure!(self.downloads.initialized, "Дождитесь загрузки очереди.");
        if let Some(id) = replacing {
            let old = self
                .downloads
                .rows
                .iter()
                .find(|j| j.id == id)
                .context("Загрузка больше не существует.")?;
            ensure!(
                self.downloads.active.as_deref() != Some(id),
                "Сначала отмените текущую загрузку."
            );
            ensure!(
                old.status != Status::Done,
                "Скачанную запись можно добавить на стол и изменить там."
            );
            job.id = id.into();
            job.created = old.created;
        }
        downloads::save(&self.data_directory(), &job)?;
        self.downloads.rows.retain(|row| row.id != job.id);
        self.downloads.rows.insert(0, job);
        self.view.dialog = Some(Dialog::Downloads(DownloadsView::default()));
        Ok(())
    }
    pub(super) fn tick_downloads(&mut self) -> Result<()> {
        if let Some(completed) = self.downloads.aux.poll() {
            self.start_download_aux();
            if completed.current {
                match completed.result {
                    Ok(Response::Loaded(config, rows)) => {
                        self.downloads.config = config;
                        self.downloads.pending_import = rows
                            .iter()
                            .filter(|r| {
                                r.status == Status::Done && !r.imported && r.error.is_none()
                            })
                            .map(|r| r.id.clone())
                            .collect();
                        self.downloads.rows = rows;
                        self.downloads.initialized = true;
                    }
                    Ok(Response::Found(rows)) => self.downloads.results = rows,
                    Ok(Response::Configured(config)) => {
                        self.downloads.config = config;
                        self.message(
                            self.text("Download settings saved", "Настройки загрузки сохранены")
                                .into(),
                        );
                    }
                    Ok(Response::Installed) => self.message(
                        self.text(
                            "Download components installed",
                            "Компоненты загрузки установлены",
                        )
                        .into(),
                    ),
                    Err(error) => self.downloads.error = Some(format!("{error:#}")),
                }
            }
        }
        if let Some(result) = self
            .downloads
            .job
            .as_mut()
            .and_then(|job| poll(&mut job.receiver))
        {
            let active = self.downloads.job.take().expect("download job");
            self.downloads.active = None;
            match result {
                Ok(job) => {
                    if job.status == Status::Done {
                        self.downloads.pending_import.push_back(job.id.clone());
                        self.message(
                            self.text(
                                "Downloaded. Adding to the sorting desk…",
                                "Скачано. Добавление на сортировочный стол…",
                            )
                            .into(),
                        );
                    }
                    self.downloads.error = job.error.clone();
                    if let Some(row) = self.downloads.rows.iter_mut().find(|r| r.id == job.id) {
                        *row = job;
                    }
                }
                Err(error) => {
                    let data = self.data_directory();
                    if let Some(row) = self.downloads.rows.iter_mut().find(|r| r.id == active.id) {
                        row.status = Status::Failed;
                        row.error = Some(format!("{error:#}"));
                        downloads::save(&data, row)?;
                    }
                    self.downloads.error = Some(format!("{error:#}"));
                }
            }
        }
        if self.downloads.initialized && self.downloads.job.is_none() {
            if let Some(index) = self
                .downloads
                .rows
                .iter()
                .position(|r| r.status == Status::Queued)
            {
                let mut row = self.downloads.rows[index].clone();
                row.status = Status::Running;
                row.error = None;
                downloads::save(&self.data_directory(), &row)?;
                self.downloads.rows[index] = row.clone();
                let data = self.data_directory();
                let config = self.downloads.config.clone();
                let cancel = Arc::new(AtomicBool::new(false));
                let worker_cancel = cancel.clone();
                let progress = self.downloads.progress.clone();
                progress.store(0, Ordering::Relaxed);
                let id = row.id.clone();
                let receiver = background(self.runtime(), move || {
                    Ok(downloads::download(
                        &data,
                        &config,
                        row,
                        &worker_cancel,
                        &progress,
                    ))
                });
                self.downloads.active = Some(id.clone());
                self.downloads.job = Some(Job {
                    id,
                    receiver,
                    cancel,
                });
            }
        }
        if self.library.import.is_none() && !self.database_busy() {
            if let Some(id) = self.downloads.importing.take() {
                let desk = self
                    .library
                    .playlists
                    .iter()
                    .find(|p| p.kind == PlaylistKind::SortingDesk);
                let data = self.data_directory();
                if let Some(row) = self.downloads.rows.iter_mut().find(|r| r.id == id) {
                    row.imported = row.path.as_ref().is_some_and(|path| {
                        desk.is_some_and(|p| p.entries.iter().any(|e| &e.track.path == path))
                    });
                    if !row.imported {
                        row.error=Some("Файл скачан. Не удалось добавить его в библиотеку; нажмите На стол для повтора.".into());
                    }
                    downloads::save(&data, row)?;
                }
            }
            if !self.busy()
                && let Some(id) = self.downloads.pending_import.pop_front()
            {
                if let Some(path) = self
                    .downloads
                    .rows
                    .iter()
                    .find(|r| r.id == id)
                    .and_then(|r| r.path.clone())
                {
                    let desk = self
                        .library
                        .playlists
                        .iter()
                        .find(|p| p.kind == PlaylistKind::SortingDesk)
                        .map(|p| p.id);
                    match self.start_dropped_import(vec![path], desk) {
                        Ok(()) => self.downloads.importing = Some(id),
                        Err(error) => self.downloads.error = Some(format!("{error:#}")),
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn downloads_scroll(&mut self, direction: i64) {
        if let Some(Dialog::Downloads(view)) = &mut self.view.dialog {
            view.selected = super::bounded(
                view.selected,
                direction,
                if view.search {
                    self.downloads.results.len()
                } else {
                    self.downloads.rows.len()
                },
            );
        }
    }
    pub(super) fn downloads_close(&mut self) -> bool {
        let Some(Dialog::Downloads(view)) = &mut self.view.dialog else {
            return false;
        };
        if view.remove.take().is_some() {
            return true;
        }
        if view.search {
            view.search = false;
            view.selected = 0;
            view.offset = 0;
        } else {
            self.view.dialog = None;
        }
        true
    }
    pub(super) fn downloads_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Downloads(view)) = &self.view.dialog else {
            return Ok(());
        };
        if view.remove.is_some() {
            return match key.key {
                Key::Char('d') => self.download_action(DownloadAction::ConfirmRemove),
                Key::Enter => self.download_action(DownloadAction::Back),
                _ => Ok(()),
            };
        }
        match key.key {
            Key::Up => self.downloads_scroll(-1),
            Key::Down => self.downloads_scroll(1),
            Key::PageUp => self.downloads_scroll(-10),
            Key::PageDown => self.downloads_scroll(10),
            Key::Enter => self.download_action(DownloadAction::Open)?,
            Key::Char('a') => self.download_action(DownloadAction::New)?,
            Key::Char('f') => self.download_action(DownloadAction::Search)?,
            Key::Char('e') => self.download_action(DownloadAction::Edit)?,
            Key::Char('s') => self.download_action(DownloadAction::Settings)?,
            Key::Char('r') => self.download_action(DownloadAction::Retry)?,
            Key::Char('c') => self.download_action(DownloadAction::Cancel)?,
            Key::Delete => self.download_action(DownloadAction::Remove)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn download_action(&mut self, action: DownloadAction) -> Result<()> {
        if let Err(error) = self.download_action_inner(action) {
            self.downloads.error = Some(format!("{error:#}"));
        }
        Ok(())
    }
    fn download_action_inner(&mut self, action: DownloadAction) -> Result<()> {
        if matches!(action, DownloadAction::Back) {
            self.downloads_close();
            return Ok(());
        }
        let Some(Dialog::Downloads(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        if let DownloadAction::Row(index) = action {
            view.selected = index;
            return Ok(());
        }
        let selected = view.selected;
        let search = view.search;
        match action {
            DownloadAction::History => {
                view.search = false;
                view.selected = 0;
                view.offset = 0;
            }
            DownloadAction::Search => {
                self.open_form(
                    self.text("Find music", "Найти музыку"),
                    self.text(
                        "Search by artist or song title.",
                        "Ищите по исполнителю или названию композиции.",
                    ),
                    vec![FormField::new(
                        self.text("Search", "Поиск"),
                        self.downloads.query.clone(),
                    )],
                    FormPurpose::DownloadSearch,
                );
            }
            DownloadAction::New
            | DownloadAction::Edit
            | DownloadAction::Queue
            | DownloadAction::Open => {
                if !search && matches!(action, DownloadAction::Open) {
                    return self.download_action_inner(DownloadAction::Import);
                }
                let old = (!search && matches!(action, DownloadAction::Edit))
                    .then(|| self.downloads.rows.get(selected).cloned())
                    .flatten();
                if let Some(old) = &old {
                    ensure!(
                        self.downloads.active.as_deref() != Some(&old.id),
                        "Сначала отмените текущую загрузку."
                    );
                    ensure!(
                        old.status != Status::Done,
                        "Измените скачанную композицию на сортировочном столе."
                    );
                }
                let hit = (search && !matches!(action, DownloadAction::New))
                    .then(|| self.downloads.results.get(selected).cloned())
                    .flatten();
                let url = old
                    .as_ref()
                    .map(|r| r.url.clone())
                    .or_else(|| hit.as_ref().map(|h| h.url.clone()))
                    .unwrap_or_default();
                let name = old
                    .as_ref()
                    .map(|r| r.name.clone())
                    .or_else(|| hit.as_ref().map(|h| h.title.clone()))
                    .unwrap_or_default();
                let folder = old.as_ref().map_or_else(
                    || self.downloads.config.directory.clone(),
                    |r| r.folder.clone(),
                );
                self.open_form(
                    self.text("Download music", "Скачать музыку"),
                    self.text(
                        "The file will be saved and added to the sorting desk.",
                        "Файл будет сохранен и добавлен на сортировочный стол.",
                    ),
                    vec![
                        FormField::new(self.text("Link", "Ссылка"), url),
                        FormField::new(self.text("Folder", "Папка"), folder.display().to_string()),
                        FormField::new(
                            self.text(
                                "File name without extension (optional)",
                                "Имя без расширения (необязательно)",
                            ),
                            name,
                        ),
                    ],
                    FormPurpose::DownloadNew(old.map(|r| r.id)),
                );
            }
            DownloadAction::Settings => {
                let mut key = FormField::new(
                    self.text(
                        "Cobalt API key (empty keeps saved)",
                        "Ключ Cobalt (пустое поле сохраняет прежний)",
                    ),
                    "",
                );
                key.secret = true;
                self.open_form(self.text("Download settings", "Настройки загрузок"),self.text("Choose a folder. Cobalt is optional and needs an API address provided by its owner.", "Выберите папку. Для Cobalt нужен адрес API, предоставленный владельцем сервера."),vec![
                    FormField::new(self.text("Download folder", "Папка загрузки"),self.downloads.config.directory.display().to_string()),
                    FormField::new(self.text("Cobalt API address (optional)", "Адрес API Cobalt (необязательно)"),self.downloads.config.cobalt_url.clone()),key,
                ],FormPurpose::DownloadSettings);
            }
            DownloadAction::Engine => {
                let mut config = self.downloads.config.clone();
                config.engine = if config.engine == Engine::YtDlp {
                    Engine::Cobalt
                } else {
                    Engine::YtDlp
                };
                ensure!(
                    config.engine != Engine::Cobalt || !config.cobalt_url.is_empty(),
                    "Сначала укажите адрес Cobalt в настройках загрузок."
                );
                self.downloads_request(Auxiliary::Configure(config, None));
            }
            DownloadAction::Tools => self.downloads_request(Auxiliary::Tools),
            DownloadAction::Cancel => {
                if search || self.downloads_loading() {
                    if let Some(cancel) = &self.downloads.cancel {
                        cancel.store(true, Ordering::Relaxed);
                    }
                } else if let Some(row) = self.downloads.rows.get_mut(selected) {
                    if let Some(job) = &self.downloads.job
                        && job.id == row.id
                    {
                        job.cancel.store(true, Ordering::Relaxed);
                    } else if row.status == Status::Queued {
                        row.status = Status::Cancelled;
                        let row = row.clone();
                        downloads::save(&self.data_directory(), &row)?;
                    }
                }
            }
            DownloadAction::Retry => {
                if !self.downloads.initialized {
                    self.initialize_downloads();
                    return Ok(());
                }
                let row = self
                    .downloads
                    .rows
                    .get(selected)
                    .context("Выберите загрузку.")?
                    .clone();
                ensure!(
                    matches!(row.status, Status::Failed | Status::Cancelled),
                    "Повтор доступен для прерванных загрузок."
                );
                let mut retry = row.clone();
                retry.status = Status::Queued;
                retry.error = None;
                retry.path = None;
                retry.expected_digest = None;
                retry.expected_size = None;
                self.queue_download(retry, Some(&row.id))?;
            }
            DownloadAction::Remove => {
                let row = self
                    .downloads
                    .rows
                    .get(selected)
                    .context("Выберите загрузку.")?;
                ensure!(
                    self.downloads.active.as_deref() != Some(&row.id),
                    "Сначала отмените текущую загрузку."
                );
                view.remove = Some(row.id.clone());
            }
            DownloadAction::ConfirmRemove => {
                let id = view.remove.clone().context("Выберите загрузку.")?;
                ensure!(
                    self.downloads.active.as_deref() != Some(&id),
                    "Дождитесь отмены загрузки."
                );
                downloads::remove(&self.data_directory(), &id)?;
                self.downloads.rows.retain(|r| r.id != id);
                self.downloads.pending_import.retain(|r| r != &id);
                if let Some(Dialog::Downloads(view)) = &mut self.view.dialog {
                    view.remove = None;
                    view.selected = 0;
                    view.offset = 0;
                }
            }
            DownloadAction::Import => {
                let row = self
                    .downloads
                    .rows
                    .get(selected)
                    .context("Выберите скачанную запись.")?;
                ensure!(row.status == Status::Done, "Дождитесь завершения загрузки.");
                self.downloads.pending_import.push_back(row.id.clone());
            }
            DownloadAction::Back | DownloadAction::Row(_) => {}
        }
        Ok(())
    }
}
