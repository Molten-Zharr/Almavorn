//! Session-bus MPRIS service shared by the graphical and terminal players.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use zbus::{
    Connection,
    connection::Builder,
    fdo,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.almavorn";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrackInfo {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub url: Option<String>,
    pub length: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub track: Option<TrackInfo>,
    pub status: &'static str,
    pub position: i64,
    pub seek_serial: u64,
    pub rate: f64,
    pub volume: f64,
    pub loop_status: &'static str,
    pub shuffle: bool,
    pub can_play: bool,
    pub can_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}

impl Snapshot {
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let Some(track) = &self.track else {
            return HashMap::new();
        };
        let mut values = HashMap::from([
            (
                "mpris:trackid".into(),
                owned(Value::from(
                    OwnedObjectPath::try_from(track.id.clone()).expect("internal track path"),
                )),
            ),
            ("mpris:length".into(), OwnedValue::from(track.length)),
            (
                "xesam:title".into(),
                owned(Value::from(track.title.clone())),
            ),
        ]);
        if !track.artist.is_empty() {
            values.insert(
                "xesam:artist".into(),
                owned(Value::from(vec![track.artist.clone()])),
            );
        }
        if !track.album.is_empty() {
            values.insert(
                "xesam:album".into(),
                owned(Value::from(track.album.clone())),
            );
        }
        if let Some(url) = &track.url {
            values.insert("xesam:url".into(), owned(Value::from(url.clone())));
        }
        values
    }
}

pub(crate) enum Command {
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    Quit,
    Seek(i64),
    SetPosition(String, i64),
    Volume(f64),
    Loop(String),
    Shuffle(bool),
    Rate(f64),
}

pub(crate) struct Incoming {
    pub command: Command,
    pub reply: oneshot::Sender<fdo::Result<()>>,
}

#[derive(Clone, Debug)]
pub(crate) enum Status {
    Connecting,
    Connected(String),
    Unavailable(String),
}

pub(crate) struct Bridge {
    pub commands: mpsc::Receiver<Incoming>,
    pub status: watch::Receiver<Status>,
    state: watch::Sender<Snapshot>,
    next_update: Instant,
    task: JoinHandle<()>,
}

impl Bridge {
    pub fn start(runtime: &Runtime, snapshot: Snapshot) -> Self {
        let (sender, commands) = mpsc::channel(32);
        let (state, receiver) = watch::channel(snapshot);
        let (status_sender, status) = watch::channel(Status::Connecting);
        let task = runtime.spawn(async move {
            loop {
                let result = serve(sender.clone(), receiver.clone(), &status_sender).await;
                if receiver.has_changed().is_err() {
                    break;
                }
                if let Err(error) = result {
                    status_sender.send_replace(Status::Unavailable(error.to_string()));
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        Self {
            commands,
            status,
            state,
            next_update: Instant::now(),
            task,
        }
    }

    pub fn due(&self) -> bool {
        Instant::now() >= self.next_update
    }

    pub fn publish(&mut self, snapshot: Snapshot) {
        self.state.send_replace(snapshot);
        self.next_update = Instant::now() + Duration::from_millis(100);
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone)]
struct Interface {
    commands: mpsc::Sender<Incoming>,
    state: watch::Receiver<Snapshot>,
}

impl Interface {
    async fn call(&self, command: Command) -> fdo::Result<()> {
        let (reply, receiver) = oneshot::channel();
        self.commands
            .try_send(Incoming { command, reply })
            .map_err(|_| fdo::Error::LimitsExceeded("Player command queue is busy".into()))?;
        tokio::time::timeout(Duration::from_secs(3), receiver)
            .await
            .map_err(|_| fdo::Error::Timeout("Player did not respond".into()))?
            .map_err(|_| fdo::Error::Failed("Player is shutting down".into()))?
    }
}

struct Root(Interface);

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    async fn quit(&self) -> fdo::Result<()> {
        self.0.call(Command::Quit).await
    }
    fn raise(&self) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported(
            "Window activation is managed by the desktop".into(),
        ))
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_quit(&self) -> bool {
        true
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_raise(&self) -> bool {
        false
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn has_track_list(&self) -> bool {
        false
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn identity(&self) -> &str {
        "Almavorn"
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn desktop_entry(&self) -> &str {
        "almavorn"
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Player(Interface);

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player", spawn = false)]
impl Player {
    #[zbus(signal)]
    async fn seeked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        position: i64,
    ) -> zbus::Result<()>;

    async fn next(&self) -> fdo::Result<()> {
        self.0.call(Command::Next).await
    }
    async fn previous(&self) -> fdo::Result<()> {
        self.0.call(Command::Previous).await
    }
    async fn pause(&self) -> fdo::Result<()> {
        self.0.call(Command::Pause).await
    }
    async fn play_pause(&self) -> fdo::Result<()> {
        self.0.call(Command::PlayPause).await
    }
    async fn stop(&self) -> fdo::Result<()> {
        self.0.call(Command::Stop).await
    }
    async fn play(&self) -> fdo::Result<()> {
        self.0.call(Command::Play).await
    }
    async fn seek(&self, offset: i64) -> fdo::Result<()> {
        self.0.call(Command::Seek(offset)).await
    }
    async fn set_position(&self, track_id: OwnedObjectPath, position: i64) -> fdo::Result<()> {
        self.0
            .call(Command::SetPosition(track_id.to_string(), position))
            .await
    }
    fn open_uri(&self, _uri: &str) -> fdo::Result<()> {
        Err(fdo::Error::NotSupported(
            "Use Almavorn's Add menu to import music".into(),
        ))
    }
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.0.state.borrow().status.into()
    }
    #[zbus(property)]
    fn loop_status(&self) -> String {
        self.0.state.borrow().loop_status.into()
    }
    #[zbus(property)]
    async fn set_loop_status(&self, value: String) -> fdo::Result<()> {
        if !matches!(value.as_str(), "None" | "Track" | "Playlist") {
            return Err(fdo::Error::InvalidArgs(
                "LoopStatus must be None, Track or Playlist".into(),
            ));
        }
        self.0.call(Command::Loop(value)).await
    }
    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.0.state.borrow().rate
    }
    #[zbus(property)]
    async fn set_rate(&self, value: f64) -> fdo::Result<()> {
        if !value.is_finite() || value.abs() > 4.0 {
            return Err(fdo::Error::InvalidArgs(
                "Rate must be between -4 and 4; 0 pauses playback".into(),
            ));
        }
        self.0.call(Command::Rate(value)).await
    }
    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.0.state.borrow().shuffle
    }
    #[zbus(property)]
    async fn set_shuffle(&self, value: bool) -> fdo::Result<()> {
        self.0.call(Command::Shuffle(value)).await
    }
    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        self.0.state.borrow().metadata()
    }
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.0.state.borrow().volume
    }
    #[zbus(property)]
    async fn set_volume(&self, value: f64) -> fdo::Result<()> {
        if !value.is_finite() {
            return Err(fdo::Error::InvalidArgs("Volume must be finite".into()));
        }
        self.0.call(Command::Volume(value)).await
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.0.state.borrow().position
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn minimum_rate(&self) -> f64 {
        -4.0
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn maximum_rate(&self) -> f64 {
        4.0
    }
    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.state.borrow().can_next
    }
    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.state.borrow().can_previous
    }
    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.0.state.borrow().can_play
    }
    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.0.state.borrow().can_pause
    }
    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.state.borrow().can_seek
    }
    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

async fn connect(interface: Interface) -> zbus::Result<(Connection, String)> {
    let connection = Builder::session()?
        .method_timeout(Duration::from_secs(3))
        .serve_at(PATH, Root(interface.clone()))?
        .serve_at(PATH, Player(interface))?
        .build()
        .await?;
    // Give every instance an exact playerctl selector, including the first one.
    // The short selector "almavorn" intentionally matches all its instances.
    let name = format!("{NAME}.instance{}", std::process::id());
    let flags = zbus::fdo::RequestNameFlags::DoNotQueue.into();
    connection
        .request_name_with_flags(name.as_str(), flags)
        .await?;
    Ok((connection, name))
}

async fn serve(
    commands: mpsc::Sender<Incoming>,
    mut state: watch::Receiver<Snapshot>,
    status: &watch::Sender<Status>,
) -> zbus::Result<()> {
    let (connection, name) = tokio::time::timeout(
        Duration::from_secs(5),
        connect(Interface {
            commands,
            state: state.clone(),
        }),
    )
    .await
    .map_err(|_| {
        zbus::Error::from(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "D-Bus connection timed out",
        ))
    })??;
    status.send_replace(Status::Connected(name));
    let mut previous = state.borrow_and_update().clone();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            changed = state.changed() => {
                if changed.is_err() { return Ok(()); }
                let current = state.borrow_and_update().clone();
                let mut properties: HashMap<&str, OwnedValue> = HashMap::new();
                macro_rules! property {
                    ($field:ident, $name:literal) => {
                        if current.$field != previous.$field {
                            properties.insert($name, OwnedValue::from(current.$field));
                        }
                    };
                }
                if current.status != previous.status { properties.insert("PlaybackStatus", owned(Value::from(current.status))); }
                if current.loop_status != previous.loop_status { properties.insert("LoopStatus", owned(Value::from(current.loop_status))); }
                if current.track != previous.track { properties.insert("Metadata", OwnedValue::from(current.metadata())); }
                property!(volume, "Volume");
                property!(rate, "Rate");
                property!(shuffle, "Shuffle");
                property!(can_play, "CanPlay");
                property!(can_pause, "CanPause");
                property!(can_next, "CanGoNext");
                property!(can_previous, "CanGoPrevious");
                property!(can_seek, "CanSeek");
                if !properties.is_empty() {
                    connection.emit_signal(None::<&str>, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", &(PLAYER, properties, Vec::<String>::new())).await?;
                }
                if current.seek_serial != previous.seek_serial
                    && current.track.as_ref().map(|track| &track.id) == previous.track.as_ref().map(|track| &track.id)
                {
                    connection.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(current.position,)).await?;
                }
                previous = current;
            }
            _ = heartbeat.tick() => {
                connection.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetId", &()).await?;
            }
        }
    }
}

fn owned(value: Value<'_>) -> OwnedValue {
    value
        .try_to_owned()
        .expect("MPRIS metadata contains no file descriptors")
}
