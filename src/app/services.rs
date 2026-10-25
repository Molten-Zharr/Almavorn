use super::{
    App, Dialog, FormField, FormPurpose,
    background::{LatestJob, background},
};
use crate::{
    audio::{Audio, PreparedAudio, StreamStatus},
    input::{Key, KeyPress},
    services::{self, Connection, Device, DeviceCode, Provider, RemoteState, Track},
};
use anyhow::{Context, Result, ensure};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

mod session;
pub(super) use session::ServiceSession;

#[derive(Clone)]
enum Operation {
    Load,
    Browse(Connection, String, Option<String>, u32),
    Save(Connection),
    Remove(Connection),
    Logout(Connection),
    Install,
    Search(Connection, Option<String>, u32),
    Login(Connection),
    Poll(Connection, String),
    YouTube(Connection, String),
    Devices(Connection),
}
#[derive(Clone)]
struct Request {
    operation: Operation,
    cancel: Arc<AtomicBool>,
}
enum Response {
    Connections(Vec<Connection>),
    Tracks(String, Vec<Track>),
    Playlists(Vec<services::Playlist>),
    Device(Connection, DeviceCode),
    Pending,
    LoggedIn,
    Installed,
    Devices(Vec<Device>),
}
#[derive(Clone)]
struct PlayRequest {
    connection: Connection,
    track: Option<Track>,
    queue: Vec<Track>,
    index: usize,
    position: u64,
    paused: bool,
    remote: Option<String>,
    device: Option<String>,
    activate: bool,
    cancel: Arc<AtomicBool>,
}
enum PlayResponse {
    Native(PreparedAudio, StreamStatus),
    Remote(RemoteState),
}
struct Native {
    connection: Connection,
    queue: Vec<Track>,
    index: usize,
    status: StreamStatus,
}
struct Authorization {
    connection: Connection,
    code: DeviceCode,
    deadline: Instant,
    next: Instant,
}

pub struct ServicesState {
    pub connections: Vec<Connection>,
    pub tracks: Vec<Track>,
    pub playlists: Vec<services::Playlist>,
    pub library_kind: String,
    pub collection: Option<String>,
    pub connection: Option<String>,
    pub query: Option<String>,
    pub page: u32,
    pub devices: Vec<Device>,
    pub selected_device: Option<String>,
    remote_device: Option<String>,
    devices_connection: Option<String>,
    pub error: Option<String>,
    pub link: Arc<Mutex<Option<String>>>,
    pub login_code: Option<String>,
    pub remote: Option<RemoteState>,
    aux: LatestJob<Request, Response>,
    cancel: Option<Arc<AtomicBool>>,
    play: LatestJob<PlayRequest, PlayResponse>,
    play_cancel: Option<Arc<AtomicBool>>,
    native: Option<Native>,
    remote_connection: Option<Connection>,
    authorization: Option<Authorization>,
    remote_updated: Instant,
}
impl Default for ServicesState {
    fn default() -> Self {
        Self {
            connections: vec![],
            tracks: vec![],
            playlists: vec![],
            library_kind: "library".into(),
            collection: None,
            connection: None,
            query: None,
            page: 0,
            devices: vec![],
            selected_device: None,
            remote_device: None,
            devices_connection: None,
            error: None,
            link: Arc::new(Mutex::new(None)),
            login_code: None,
            remote: None,
            aux: LatestJob::default(),
            cancel: None,
            play: LatestJob::default(),
            play_cancel: None,
            native: None,
            remote_connection: None,
            authorization: None,
            remote_updated: Instant::now(),
        }
    }
}
impl ServicesState {
    pub(super) fn select_connection(&mut self, id: String) {
        if self.connection.as_ref() != Some(&id) {
            if self.aux.requested().is_some_and(|request| matches!(
                &request.operation, Operation::Browse(..) | Operation::Search(..) | Operation::Devices(..)
            )) {
                if let Some(cancel) = &self.cancel { cancel.store(true, Ordering::Relaxed); }
                self.aux.cancel();
            }
            self.tracks.clear();
            self.playlists.clear();
            self.devices.clear();
            self.selected_device = None;
            self.devices_connection = None;
            self.query = None;
            self.collection = None;
            self.library_kind = "library".into();
            self.page = 0;
        }
        self.connection = Some(id);
    }

    pub(super) fn cancel_all(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.play_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for ServicesState {
    fn drop(&mut self) {
        self.cancel_all();
    }
}
#[derive(Clone, Default)]
pub struct ServicesView {
    pub page: ServicePage,
    pub selected: usize,
    pub offset: usize,
    pub remove: Option<Connection>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum ServicePage {
    #[default]
    Connections,
    Tracks,
    Devices,
    Playlists,
}
#[derive(Clone, Copy)]
pub enum ServiceAction {
    Guide,
    Row(usize),
    Open,
    Add(Provider),
    Edit,
    Remove,
    ConfirmRemove,
    Enable,
    Login,
    Logout,
    Search,
    Library,
    Liked,
    Playlists,
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    Devices,
    SelectDevice,
    Install,
    Tools,
    OpenLink,
    CopyLink,
    Cancel,
    Page(i32),
    Back,
}

impl App {
    pub fn services_loading(&self) -> bool {
        self.services.aux.requested().is_some()
    }
    pub fn services_playing(&self) -> bool {
        self.services.native.is_some() || self.services.remote_connection.is_some()
    }
    pub(super) fn open_services(&mut self) {
        self.open_settings_page(super::SettingsPage::Services);
        self.view.settings.focus = super::SettingsFocus::Parameters;
    }
    pub(super) fn ensure_services_loaded(&mut self) {
        if self.services.connections.is_empty() && !self.services_loading() {
            self.service_request(Operation::Load);
        }
    }
    fn service_request(&mut self, operation: Operation) {
        let changed = match &operation {
            Operation::Remove(c) | Operation::Logout(c) => Some(c),
            Operation::Save(c) if !c.enabled => Some(c),
            _ => None,
        };
        if let Some(connection) = changed {
            let active = self
                .services
                .native
                .as_ref()
                .is_some_and(|n| n.connection.id == connection.id)
                || self
                    .services
                    .remote_connection
                    .as_ref()
                    .is_some_and(|r| r.id == connection.id);
            if active {
                self.stop_playback();
            }
        }

        if let Some(cancel) = self.services.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.services.cancel = Some(cancel.clone());
        self.services.error = None;
        self.services.aux.request(Request { operation, cancel });
        self.start_service_job();
    }
    fn start_service_job(&mut self) {
        let credentials = self.services.aux.requested().is_some_and(|r| {
            matches!(
                &r.operation,
                Operation::Save(_)
                    | Operation::Remove(_)
                    | Operation::Logout(_)
                    | Operation::Login(_)
                    | Operation::YouTube(_, _)
            )
        });
        if credentials
            && (self.services.play.is_running() || self.services.play.requested().is_some())
        {
            return;
        }

        let data = self.data_directory();
        let link = self.services.link.clone();
        let runtime = self.runtime.as_ref().expect("runtime");
        self.services.aux.start(|request| {
            let request = request.clone();
            background(runtime, move || {
                let cancel = &request.cancel;
                ensure!(!cancel.load(Ordering::Relaxed), "Операция отменена.");
                match request.operation {
                    Operation::Browse(connection, kind, playlist, page) => {
                        let value = services::library(
                            &data,
                            &connection,
                            &kind,
                            playlist.as_deref(),
                            page,
                            cancel,
                        )?;
                        if kind == "playlists" {
                            Ok(Response::Playlists(serde_json::from_value(
                                value["playlists"].clone(),
                            )?))
                        } else {
                            Ok(Response::Tracks(
                                connection.id,
                                serde_json::from_value(value["tracks"].clone())?,
                            ))
                        }
                    }
                    Operation::Load => Ok(Response::Connections(services::list(&data)?)),
                    Operation::Save(connection) => {
                        services::save(&data, &connection)?;
                        Ok(Response::Connections(services::list(&data)?))
                    }
                    Operation::Remove(connection) => {
                        services::remove(&data, &connection)?;
                        Ok(Response::Connections(services::list(&data)?))
                    }
                    Operation::Logout(connection) => {
                        services::logout(&data, &connection)?;
                        Ok(Response::Connections(services::list(&data)?))
                    }
                    Operation::Install => {
                        services::install(&data, cancel)?;
                        Ok(Response::Installed)
                    }
                    Operation::Search(connection, query, page) => Ok(Response::Tracks(
                        connection.id.clone(),
                        services::search(&data, &connection, query.as_deref(), page, cancel)?,
                    )),
                    Operation::Login(connection) => {
                        if connection.provider == Provider::Yandex {
                            let code = services::device_code(&data, &connection, cancel)?;
                            Ok(Response::Device(connection, code))
                        } else {
                            services::spotify_login(&data, &connection, cancel, |value| {
                                if let Ok(mut link) = link.lock() {
                                    *link = Some(value);
                                }
                            })?;
                            Ok(Response::LoggedIn)
                        }
                    }
                    Operation::Poll(connection, code) => Ok(
                        if services::device_poll(&data, &connection, &code, cancel)? {
                            Response::LoggedIn
                        } else {
                            Response::Pending
                        },
                    ),
                    Operation::YouTube(connection, headers) => {
                        services::youtube_login(&data, &connection, &headers, cancel)?;
                        Ok(Response::LoggedIn)
                    }
                    Operation::Devices(connection) => Ok(Response::Devices(services::devices(
                        &data,
                        &connection,
                        cancel,
                    )?)),
                }
            })
        });
    }
    pub(super) fn service_save(&mut self, connection: Connection) {
        self.services.select_connection(connection.id.clone());
        self.view.dialog = Some(Dialog::Services(ServicesView::default()));
        self.service_request(Operation::Save(connection));
    }
    pub(super) fn service_headers(&mut self, connection: Connection, headers: String) {
        self.services.select_connection(connection.id.clone());
        self.view.dialog = Some(Dialog::Services(ServicesView::default()));
        self.service_request(Operation::YouTube(connection, headers));
    }
    pub(super) fn service_search(&mut self, query: Option<String>, page: u32) -> Result<()> {
        let connection = self.service_connection()?;
        if let Some(query) = &query {
            ensure!(
                !query.trim().is_empty(),
                "Введите название или исполнителя."
            );
        }
        self.services.query = query.clone();
        self.services.page = page;
        self.services.tracks.clear();
        self.view.dialog = Some(Dialog::Services(ServicesView {
            page: ServicePage::Tracks,
            ..Default::default()
        }));
        if query.is_none() {
            self.service_request(Operation::Browse(
                connection,
                self.services.library_kind.clone(),
                self.services.collection.clone(),
                page,
            ));
        } else {
            self.service_request(Operation::Search(connection, query, page));
        }
        Ok(())
    }
    fn service_playlists(&mut self, page: u32) -> Result<()> {
        let connection = self.service_connection()?;
        self.services.page = page;
        self.services.playlists.clear();
        self.view.dialog = Some(Dialog::Services(ServicesView {
            page: ServicePage::Playlists,
            ..Default::default()
        }));
        self.service_request(Operation::Browse(
            connection,
            "playlists".into(),
            None,
            page,
        ));
        Ok(())
    }
    fn service_connection(&self) -> Result<Connection> {
        self.services
            .connections
            .iter()
            .find(|c| Some(&c.id) == self.services.connection.as_ref())
            .cloned()
            .context("Выберите подключение к сервису.")
    }
    pub(super) fn tick_services(&mut self) -> Result<()> {
        if let Some(completed) = self.services.aux.poll() {
            self.start_service_job();
            if completed.current {
                match completed.result {
                    Ok(Response::Connections(rows)) => {
                        self.services.connections = rows;
                        if let Some(Dialog::Services(view)) = &mut self.view.dialog
                            && view.page == ServicePage::Connections
                        {
                            view.selected = self.services.connections.iter()
                                .position(|c| Some(&c.id) == self.services.connection.as_ref())
                                .unwrap_or(0);
                        }
                    }
                    Ok(Response::Playlists(rows)) => {
                        self.services.playlists = rows;
                    }
                    Ok(Response::Tracks(id, rows)) => {
                        if self.services.connection.as_ref() == Some(&id) {
                            self.services.tracks = rows;
                        }
                    }
                    Ok(Response::Device(connection, code)) => {
                        self.services.login_code = Some(code.user_code.clone());
                        if let Ok(mut link) = self.services.link.lock() {
                            *link = Some(code.url.clone());
                        }
                        self.services.authorization = Some(Authorization {
                            connection,
                            deadline: Instant::now()
                                + Duration::from_secs(code.expires_in.min(1800)),
                            next: Instant::now() + Duration::from_secs(code.interval.max(5)),
                            code,
                        });
                    }
                    Ok(Response::Pending) => {}
                    Ok(Response::LoggedIn) => {
                        self.services.authorization = None;
                        self.services.login_code = None;
                        if let Ok(mut link) = self.services.link.lock() {
                            *link = None;
                        }
                        self.message(self.text("Account connected", "Аккаунт подключен").into());
                        self.service_request(Operation::Load);
                    }
                    Ok(Response::Installed) => self.message(
                        self.text(
                            "Service support installed",
                            "Поддержка сервисов установлена",
                        )
                        .into(),
                    ),
                    Ok(Response::Devices(devices)) => {
                        self.services.devices = devices;
                    }
                    Err(error) => {
                        self.services.error = Some(format!("{error:#}"));
                        if self.services.authorization.is_some() {
                            self.services.authorization = None;
                            self.services.login_code = None;
                        }
                    }
                }
            }
        }
        if !self.services_loading()
            && let Some(auth) = &mut self.services.authorization
        {
            if Instant::now() > auth.deadline {
                self.services.authorization = None;
                self.services.login_code = None;
                self.services.error = Some("Время входа истекло. Нажмите Войти снова.".into());
            } else if Instant::now() >= auth.next {
                auth.next = Instant::now() + Duration::from_secs(auth.code.interval.max(5));
                let operation =
                    Operation::Poll(auth.connection.clone(), auth.code.device_code.clone());
                self.service_request(operation);
            }
        }
        if let Some(completed) = self.services.play.poll() {
            self.start_service_play();
            if completed.current {
                match completed.result {
                    Ok(PlayResponse::Native(source, status)) => {
                        let request = completed.request;
                        let track = request.track.context("Композиция недоступна.")?;
                        self.stop_local_for_service();
                        if self.playback.audio.is_none() {
                            self.playback.audio =
                                Some(Audio::open(self.settings.preferences.volume)?);
                        }
                        let audio = self.playback.audio.as_mut().expect("audio");
                        audio.equalizer(&self.settings.preferences.equalizer);
                        audio.tuner(&Default::default());
                        audio.play_prepared_at(
                            source,
                            request.paused,
                            Duration::from_millis(request.position),
                        );
                        self.playback.current = Some(service_track(&request.connection, &track));
                        self.services.native = Some(Native {
                            connection: request.connection,
                            queue: request.queue,
                            index: request.index,
                            status,
                        });
                        self.services.remote_connection = None;
                        self.services.remote = None;
                        self.playback.waveform = Some(vec![]);
                        self.playback.seek_preview = None;
                        self.playback.seek_paused = None;
                    }
                    Ok(PlayResponse::Remote(state)) => {
                        let request = completed.request;
                        self.services.remote_updated = Instant::now();
                        if request.activate {
                            self.stop_local_for_service();
                            self.playback.current = state
                                .track
                                .as_ref()
                                .map(|t| service_track(&request.connection, t));
                            self.services.native = None;
                            self.services.remote_device = request.device.clone();
                            self.services.remote_connection = Some(request.connection.clone());
                        }
                        if request.activate {
                            self.services.remote = Some(state);
                        } else if self
                            .services
                            .remote_connection
                            .as_ref()
                            .is_some_and(|c| c.id == request.connection.id)
                        {
                            self.services.remote_connection = None;
                            self.services.remote = None;
                            self.playback.current = None;
                        }
                    }
                    Err(error) => {
                        if completed.request.remote.as_deref() == Some("pause")
                            && !completed.request.activate
                        {
                            self.services.aux.cancel();
                        }
                        if completed.request.remote.is_none() {
                            self.stop_local_for_service();
                            self.services.native = None;
                            self.playback.current = None;
                        }
                        self.playback.seek_preview = None;
                        self.playback.seek_paused = None;
                        self.services.error = Some(format!("{error:#}"));
                        self.failure(error);
                    }
                }
            }
        }
        self.start_service_job();
        let stream_error = self.services.native.as_ref().and_then(|native| {
            native
                .status
                .error
                .lock()
                .ok()
                .and_then(|mut error| error.take())
        });
        if let Some(message) = stream_error {
            self.stop_playback();
            self.services.error = Some(message.clone());
            self.failure(anyhow::anyhow!(message));
        }
        if self.services.remote_connection.is_some()
            && self.services.play.requested().is_none()
            && self.services.remote_updated.elapsed() > Duration::from_secs(10)
        {
            self.service_remote("state", None, 0, true)?;
        }
        Ok(())
    }
    fn stop_local_for_service(&mut self) {
        self.finish_listening();
        self.cancel_audio_preparation();
        self.playback.preparation.cancel();
        self.cancel_seek();
        self.cancel_waveform();
        if let Some(audio) = &mut self.playback.audio {
            audio.stop();
        }
        self.playback.queue = Default::default();
        self.playback.playing_entry = None;
        self.playback.playing_playlist = None;
        self.playback.playing_source = None;
    }
    fn start_service_play(&mut self) {
        let previous_remote = self.service_remote_source();
        let data = self.data_directory();
        let runtime = self.runtime.as_ref().expect("runtime");
        self.services.play.start(|request| {
            let previous_remote = previous_remote.clone();
            let request = request.clone();
            background(runtime, move || {
                ensure!(
                    !request.cancel.load(Ordering::Relaxed),
                    "Воспроизведение отменено."
                );
                if let Some(operation) = &request.remote {
                    if operation == "play"
                        && let Some((previous, device)) = &previous_remote
                        && previous.id != request.connection.id
                    {
                        services::remote(
                            &data,
                            previous,
                            device.as_deref(),
                            "pause",
                            None,
                            0,
                            &request.cancel,
                        )?;
                    }
                    return Ok(PlayResponse::Remote(services::remote(
                        &data,
                        &request.connection,
                        request.device.as_deref(),
                        operation,
                        request.track.as_ref().map(|t| t.id.as_str()),
                        request.position,
                        &request.cancel,
                    )?));
                }
                if let Some((connection, device)) = previous_remote {
                    services::remote(
                        &data,
                        &connection,
                        device.as_deref(),
                        "pause",
                        None,
                        0,
                        &request.cancel,
                    )?;
                }
                let track = request.track.as_ref().context("Выберите композицию.")?;
                let media = services::resolve(&data, &request.connection, track, &request.cancel)?;
                let (source, status) = Audio::prepare_stream(
                    media,
                    Duration::from_millis(request.position),
                    request.cancel,
                )?;
                Ok(PlayResponse::Native(source, status))
            })
        });
    }
    fn request_service_play(&mut self, mut request: PlayRequest) {
        if let Some(cancel) = self.services.play_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        request.cancel = Arc::new(AtomicBool::new(false));
        self.services.play_cancel = Some(request.cancel.clone());
        self.services.error = None;
        self.services.play.request(request);
        self.start_service_play();
    }
    pub(super) fn service_remote(
        &mut self,
        operation: &str,
        track: Option<Track>,
        number: u64,
        activate: bool,
    ) -> Result<()> {
        let connection = self
            .services
            .remote_connection
            .clone()
            .or_else(|| self.service_connection().ok())
            .context("Выберите Spotify.")?;
        self.services.remote_updated = Instant::now();
        self.request_service_play(PlayRequest {
            connection,
            track,
            queue: vec![],
            index: 0,
            position: number,
            paused: false,
            remote: Some(operation.into()),
            device: self.services.remote_device.clone(),
            activate,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        Ok(())
    }
    pub(super) fn service_remote_source(&self) -> Option<(Connection, Option<String>)> {
        self.services
            .remote_connection
            .clone()
            .map(|c| (c, self.services.remote_device.clone()))
    }
    pub(super) fn service_detach_native(&mut self) {
        if let Some(cancel) = self.services.play_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.services.play.cancel();
        self.services.native = None;
    }
    pub(super) fn service_local_started(&mut self) {
        self.services.remote_connection = None;
        self.services.remote = None;
    }
    pub(super) fn service_stop(&mut self) {
        if self.services.remote_connection.is_some() {
            let _ = self.service_remote("pause", None, 0, false);
        } else {
            if let Some(cancel) = self.services.play_cancel.take() {
                cancel.store(true, Ordering::Relaxed);
            }
            self.services.play.cancel();
        }
        self.services.native = None;
        self.services.remote_connection = None;
        self.services.remote = None;
    }
    pub(super) fn service_toggle(&mut self) -> Result<bool> {
        if let Some(request) = self.services.play.requested_mut()
            && request.remote.is_none()
        {
            request.paused = !request.paused;
            return Ok(true);
        }
        if self.services.remote_connection.is_some() {
            let playing = self.services.remote.as_ref().is_some_and(|s| s.playing);
            self.service_remote(if playing { "pause" } else { "play" }, None, 0, true)?;
            return Ok(true);
        }
        if self.services.native.is_some() {
            if let Some(audio) = &self.playback.audio {
                audio.toggle();
            }
            return Ok(true);
        }
        Ok(false)
    }
    pub(super) fn service_next(&mut self, direction: i64, explicit: bool) -> Result<bool> {
        if self.services.remote_connection.is_some() {
            self.service_remote(
                if direction < 0 { "previous" } else { "next" },
                None,
                0,
                true,
            )?;
            return Ok(true);
        }
        let Some(native) = &self.services.native else {
            return Ok(false);
        };
        let repeat = self.settings.preferences.repeat;
        let count = native.queue.len();
        let index = if !explicit && repeat == crate::model::RepeatMode::Track {
            native.index
        } else if self.settings.preferences.shuffle && count > 1 {
            let mut index = fastrand::usize(..count - 1);
            if index >= native.index {
                index += 1;
            }
            index
        } else {
            let index = native.index as i64 + direction;
            if index < 0 || index >= count as i64 {
                if repeat == crate::model::RepeatMode::Playlist {
                    index.rem_euclid(count.max(1) as i64) as usize
                } else {
                    if !explicit {
                        self.stop_playback();
                    }
                    return Ok(true);
                }
            } else {
                index as usize
            }
        };
        let request = PlayRequest {
            connection: native.connection.clone(),
            track: native.queue.get(index).cloned(),
            queue: native.queue.clone(),
            index,
            position: 0,
            paused: false,
            remote: None,
            device: None,
            activate: true,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.request_service_play(request);
        Ok(true)
    }
    pub(super) fn service_seek(&mut self, position: u64) -> bool {
        if self.services.remote_connection.is_some() {
            if let Err(error) = self.service_remote("seek", None, position, true) {
                self.failure(error);
            }
            return true;
        }
        let Some(native) = &self.services.native else {
            return false;
        };
        let paused = self
            .playback
            .seek_paused
            .unwrap_or_else(|| self.playback.audio.as_ref().is_none_or(Audio::paused));
        let request = PlayRequest {
            connection: native.connection.clone(),
            track: native.queue.get(native.index).cloned(),
            queue: native.queue.clone(),
            index: native.index,
            position,
            paused,
            remote: None,
            device: None,
            activate: true,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        self.request_service_play(request);
        true
    }
    pub(super) fn service_volume(&mut self, volume: f32) -> Result<()> {
        if self.services.remote_connection.is_some() {
            self.service_remote("volume", None, (volume * 100.).round() as u64, true)?;
        }
        Ok(())
    }
    pub(super) fn service_position(&self) -> Option<u64> {
        if self.services.remote_connection.is_some() {
            return self.services.remote.as_ref().map(|s| {
                s.progress_ms
                    + if s.playing {
                        self.services.remote_updated.elapsed().as_millis() as u64
                    } else {
                        0
                    }
            });
        }
        self.services.native.as_ref().map(|native| {
            self.playback
                .audio
                .as_ref()
                .map_or(0, |a| a.position().as_millis() as u64)
                .saturating_sub(native.status.buffering_ms())
        })
    }
    pub(super) fn service_paused(&self) -> Option<bool> {
        self.services
            .remote_connection
            .as_ref()
            .map(|_| !self.services.remote.as_ref().is_some_and(|s| s.playing))
    }
    pub(super) fn service_caption(&self) -> Option<String> {
        let connection = self
            .services
            .native
            .as_ref()
            .map(|n| &n.connection)
            .or(self.services.remote_connection.as_ref())?;
        Some(format!(
            "{}: {}{}",
            if self.current_paused() {
                self.text("Paused", "Пауза")
            } else {
                self.text("Playing", "Играет")
            },
            connection.name,
            self.services
                .remote
                .as_ref()
                .filter(|s| !s.device.is_empty())
                .map(|s| format!(" · {}", s.device))
                .unwrap_or_default()
        ))
    }
    pub fn services_remote_active(&self) -> bool {
        self.services.remote_connection.is_some()
    }
    pub(super) fn service_preparing(&self) -> bool {
        self.services
            .play
            .requested()
            .is_some_and(|p| p.remote.is_none())
    }
    pub(super) fn services_scroll(&mut self, direction: i64) {
        if let Some(Dialog::Services(view)) = &mut self.view.dialog {
            view.selected = super::bounded(
                view.selected,
                direction,
                match view.page {
                    ServicePage::Connections => self.services.connections.len(),
                    ServicePage::Tracks => self.services.tracks.len(),
                    ServicePage::Devices => self.services.devices.len(),
                    ServicePage::Playlists => self.services.playlists.len(),
                },
            );
        }
    }
    pub(super) fn services_close(&mut self) -> bool {
        let Some(Dialog::Services(view)) = &mut self.view.dialog else {
            return false;
        };
        if view.remove.take().is_some() {
            return true;
        }
        if view.page != ServicePage::Connections {
            view.page = ServicePage::Connections;
            view.selected = 0;
            view.offset = 0;
        } else {
            self.view.dialog = None;
        }
        true
    }
    pub(super) fn services_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Services(view)) = &self.view.dialog else {
            return Ok(());
        };
        if view.remove.is_some() {
            return match key.key {
                Key::Char('d') => self.service_action(ServiceAction::ConfirmRemove),
                Key::Enter => self.service_action(ServiceAction::Back),
                _ => Ok(()),
            };
        }
        match key.key {
            Key::Up => self.services_scroll(-1),
            Key::Down => self.services_scroll(1),
            Key::Home => self.services_scroll(i64::MIN),
            Key::End => self.services_scroll(i64::MAX),
            Key::PageUp => self.services_scroll(-10),
            Key::PageDown => self.services_scroll(10),
            Key::Enter => self.service_action(ServiceAction::Open)?,
            Key::Char('f') => self.service_action(ServiceAction::Search)?,
            Key::Char('e') => self.service_action(ServiceAction::Edit)?,
            Key::Delete => self.service_action(ServiceAction::Remove)?,
            Key::Char(' ') => self.service_action(ServiceAction::Pause)?,
            _ => {}
        }
        Ok(())
    }
    pub(super) fn service_action(&mut self, action: ServiceAction) -> Result<()> {
        self.view.settings.focus = super::SettingsFocus::Parameters;
        if let Err(error) = self.service_action_inner(action) {
            self.services.error = Some(format!("{error:#}"));
        }
        Ok(())
    }
    fn service_action_inner(&mut self, action: ServiceAction) -> Result<()> {
        let busy = self.services_loading();
        if matches!(action, ServiceAction::Back) {
            self.services_close();
            return Ok(());
        }
        let Some(Dialog::Services(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        if let ServiceAction::Row(index) = action {
            view.selected = index;
            return Ok(());
        }
        let page = view.page;
        let selected = view.selected;
        ensure!(
            !busy
                || matches!(
                    action,
                    ServiceAction::OpenLink
                        | ServiceAction::Guide
                        | ServiceAction::CopyLink
                        | ServiceAction::Cancel
                        | ServiceAction::Pause
                        | ServiceAction::Stop
                        | ServiceAction::Previous
                        | ServiceAction::Next
                ),
            "Дождитесь завершения операции или отмените ее."
        );
        if page == ServicePage::Connections
            && let Some(connection) = self.services.connections.get(selected)
        {
            let id = connection.id.clone();
            self.services.select_connection(id);
        }
        match action {
            ServiceAction::Add(_) | ServiceAction::Edit => {
                let connection = if let ServiceAction::Add(provider) = action {
                    Connection::new(provider)
                } else {
                    self.service_connection()?
                };
                let mut fields = vec![FormField::new(
                    self.text("Connection name", "Название подключения"),
                    connection.name.clone(),
                )];
                if connection.provider == Provider::Spotify {
                    fields.push(FormField::new(
                        "Spotify Client ID",
                        connection.client_id.clone(),
                    ));
                }
                self.open_form(self.text("Music service","Музыкальный сервис"),if connection.provider==Provider::Spotify{self.text("Spotify Premium and a developer Client ID are required. Register redirect URI: http://127.0.0.1:43827/callback", "Нужны Spotify Premium и Client ID приложения Spotify. Укажите адрес возврата: http://127.0.0.1:43827/callback")}else{self.text("Name this connection.","Назовите подключение.")},fields,FormPurpose::ServiceConnection(connection));
            }
            ServiceAction::Remove => {
                let connection = self.service_connection()?;
                if let Some(Dialog::Services(view)) = &mut self.view.dialog {
                    view.remove = Some(connection);
                }
            }
            ServiceAction::ConfirmRemove => {
                let connection = view.remove.take().context("Выберите подключение.")?;
                ensure!(
                    !self.services_loading(),
                    "Дождитесь завершения операции или отмените ее."
                );
                self.services.authorization = None;
                self.service_request(Operation::Remove(connection));
            }
            ServiceAction::Enable => {
                let mut connection = self.service_connection()?;
                connection.enabled = !connection.enabled;
                self.service_request(Operation::Save(connection));
            }
            ServiceAction::Logout => {
                let connection = self.service_connection()?;
                self.services.authorization = None;
                self.services.login_code = None;
                self.service_request(Operation::Logout(connection));
            }
            ServiceAction::Login => {
                let connection = self.service_connection()?;
                ensure!(connection.enabled, "Включите подключение.");
                self.services.authorization = None;
                self.services.login_code = None;
                if let Ok(mut link) = self.services.link.lock() {
                    *link = None;
                }
                if connection.provider == Provider::YouTube {
                    let mut headers = FormField::new(
                        self.text(
                            "Browser headers or browser.json contents",
                            "Заголовки браузера или содержимое browser.json",
                        ),
                        "",
                    );
                    headers.secret = true;
                    headers.multiline = true;
                    self.open_form(self.text("Connect YouTube Music","Подключить YouTube Music"),self.text("Open music.youtube.com and sign in. In browser developer tools, copy request headers from a browse request. Instructions: ytmusicapi.readthedocs.io/en/stable/setup/browser.html", "Откройте music.youtube.com и войдите. В инструментах разработчика браузера скопируйте заголовки запроса browse. Инструкция: ytmusicapi.readthedocs.io/en/stable/setup/browser.html"),vec![headers],FormPurpose::ServiceHeaders(connection));
                } else {
                    self.service_request(Operation::Login(connection));
                }
            }
            ServiceAction::Search => {
                self.service_connection()?;
                self.open_form(
                    self.text("Search music service", "Поиск в сервисе"),
                    self.text(
                        "Enter an artist or song title.",
                        "Введите исполнителя или название песни.",
                    ),
                    vec![FormField::new(
                        self.text("Search", "Поиск"),
                        self.services.query.clone().unwrap_or_default(),
                    )],
                    FormPurpose::ServiceSearch,
                );
            }
            ServiceAction::Library | ServiceAction::Liked => {
                self.services.collection = None;
                self.services.library_kind = if matches!(action, ServiceAction::Liked) {
                    "liked"
                } else {
                    "library"
                }
                .into();
                self.service_search(None, 0)?;
            }
            ServiceAction::Playlists => {
                self.service_playlists(0)?;
            }
            ServiceAction::Open if page == ServicePage::Playlists => {
                let playlist = self
                    .services
                    .playlists
                    .get(selected)
                    .context("Выберите плейлист.")?;
                self.services.collection = Some(playlist.id.clone());
                self.services.library_kind = "playlist".into();
                self.service_search(None, 0)?;
            }
            ServiceAction::Open if page == ServicePage::Connections => {
                return self.service_action_inner(ServiceAction::Search);
            }
            ServiceAction::Open if page == ServicePage::Devices => {
                return self.service_action_inner(ServiceAction::SelectDevice);
            }
            ServiceAction::Play | ServiceAction::Open => {
                let connection = self.service_connection()?;
                let track = self
                    .services
                    .tracks
                    .get(selected)
                    .cloned()
                    .context("Выберите композицию.")?;
                ensure!(track.available, "Композиция недоступна.");
                if connection.provider == Provider::Spotify {
                    let device = (self.services.devices_connection.as_ref()
                        == Some(&connection.id))
                    .then(|| self.services.selected_device.clone())
                    .flatten();
                    self.request_service_play(PlayRequest {
                        connection,
                        track: Some(track),
                        queue: vec![],
                        index: 0,
                        position: 0,
                        paused: false,
                        remote: Some("play".into()),
                        device,
                        activate: true,
                        cancel: Arc::new(AtomicBool::new(false)),
                    });
                } else {
                    self.request_service_play(PlayRequest {
                        connection,
                        track: Some(track),
                        queue: self.services.tracks.clone(),
                        index: selected,
                        position: 0,
                        paused: false,
                        remote: None,
                        device: None,
                        activate: true,
                        cancel: Arc::new(AtomicBool::new(false)),
                    });
                }
            }
            ServiceAction::Pause => {
                self.toggle_playback()?;
            }
            ServiceAction::Stop => self.stop_playback(),
            ServiceAction::Previous => {
                self.next(-1, true)?;
            }
            ServiceAction::Next => {
                self.next(1, true)?;
            }
            ServiceAction::Devices => {
                let connection = self.service_connection()?;
                ensure!(
                    connection.provider == Provider::Spotify,
                    "Выбор устройства доступен для Spotify Connect."
                );
                self.view.dialog = Some(Dialog::Services(ServicesView {
                    page: ServicePage::Devices,
                    ..Default::default()
                }));
                self.service_request(Operation::Devices(connection));
            }
            ServiceAction::SelectDevice => {
                let device = self
                    .services
                    .devices
                    .get(selected)
                    .context("Откройте Spotify на нужном устройстве и обновите список.")?;
                ensure!(
                    !device.is_restricted,
                    "Spotify ограничил управление этим устройством."
                );
                self.services.selected_device = device.id.clone();
                self.services.devices_connection = self.services.connection.clone();
                self.view.dialog = Some(Dialog::Services(ServicesView {
                    page: ServicePage::Tracks,
                    ..Default::default()
                }));
            }
            ServiceAction::Guide => {
                self.open_settings_page(super::SettingsPage::ServiceGuide);
                self.view.settings.focus = super::SettingsFocus::Parameters;
            }
            ServiceAction::Install => self.service_request(Operation::Install),
            ServiceAction::Tools => self.open_downloads(),
            ServiceAction::OpenLink | ServiceAction::CopyLink => {
                let link = self
                    .services
                    .link
                    .lock()
                    .ok()
                    .and_then(|v| v.clone())
                    .context("Сначала нажмите Войти.")?;
                if matches!(action, ServiceAction::CopyLink) {
                    self.view.clipboard_text = Some(link);
                } else {
                    let url = crate::network::public_url(&link)?;
                    ensure!(url.scheme() == "https", "Неверная ссылка входа.");
                    let mut child=std::process::Command::new("xdg-open")
                        .arg(url.as_str())
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .context("Не удалось открыть браузер. Скопируйте ссылку.")?;
                    std::thread::spawn(move||{let _=child.wait();});
                }
            }
            ServiceAction::Cancel => {
                if let Some(cancel) = &self.services.cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
                self.services.authorization = None;
                self.services.login_code = None;
                self.services.aux.cancel();
                if let Ok(mut link) = self.services.link.lock() {
                    *link = None;
                }
            }
            ServiceAction::Page(direction) => {
                let page = self.services.page.saturating_add_signed(direction);
                if matches!(&self.view.dialog,Some(Dialog::Services(view)) if view.page==ServicePage::Playlists)
                {
                    self.service_playlists(page)?;
                } else {
                    self.service_search(self.services.query.clone(), page)?;
                }
            }
            ServiceAction::Row(_) | ServiceAction::Back => {}
        }
        Ok(())
    }
}
fn service_track(connection: &Connection, track: &Track) -> crate::model::Track {
    crate::model::Track {
        id: service_track_id(connection, track),
        path: format!("almavorn://{}/{}", connection.id, track.id).into(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration_ms: track.duration_ms,
        tags: String::new(),
    }
}

fn service_track_id(connection: &Connection, track: &Track) -> i64 {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(connection.id.as_bytes());
    hash.update([0]);
    hash.update(track.id.as_bytes());
    let hash = hash.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash[..8]);
    -((u64::from_be_bytes(bytes) & 0x3fff_ffff_ffff_ffff) as i64) - 1
}
