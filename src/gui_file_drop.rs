use almavorn::model::Language;
use anyhow::Result;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
    backend::{Backend, ObjectId, WaylandError},
    delegate_noop, event_created_child,
    protocol::{
        wl_data_device, wl_data_device_manager, wl_data_offer, wl_data_source, wl_pointer,
        wl_registry, wl_seat, wl_surface,
    },
};

const URI_LIST: &str = "text/uri-list";
type Paths = io::Result<Vec<PathBuf>>;

pub struct DroppedFiles {
    pub playlist: Option<i64>,
    pub paths: Paths,
}

pub struct WaylandDrop {
    queue: EventQueue<State>,
    state: State,
    connection: Connection,
    origin: wl_surface::WlSurface,
}

impl WaylandDrop {
    pub fn new(context: &(impl HasDisplayHandle + HasWindowHandle)) -> Result<Option<Self>> {
        let RawDisplayHandle::Wayland(display) = context.display_handle()?.as_raw() else {
            return Ok(None);
        };
        let RawWindowHandle::Wayland(window) = context.window_handle()?.as_raw() else {
            return Ok(None);
        };
        // Eframe owns the display. PlayerUi drops this receiver in on_exit, before
        // eframe destroys its window. Workers only own sockets, never Wayland proxies.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        // Borrow eframe's surface only as a request argument. Never destroy it.
        let surface_id = unsafe {
            ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                window.surface.as_ptr().cast(),
            )?
        };
        let origin = wl_surface::WlSurface::from_id(&connection, surface_id)?;
        let queue = connection.new_event_queue();
        connection.display().get_registry(&queue.handle(), ());
        connection.flush()?;
        Ok(Some(Self {
            queue,
            state: State {
                surface: window.surface.as_ptr() as usize,
                ..Default::default()
            },
            connection,
            origin,
        }))
    }

    pub fn start_drag(&mut self, uris: Arc<[u8]>) -> Result<()> {
        self.queue.dispatch_pending(&mut self.state)?;
        anyhow::ensure!(
            self.state.outgoing.is_none(),
            "{}",
            self.state.language.text(
                "A file drag is already active",
                "Перетаскивание файлов уже выполняется"
            )
        );
        let manager = self
            .state
            .manager
            .as_ref()
            .filter(|m| m.version() >= 3)
            .ok_or_else(|| {
                anyhow::anyhow!(self.state.language.text(
                    "The desktop does not support file dragging",
                    "Рабочий стол не поддерживает перетаскивание файлов"
                ))
            })?;
        let (device, serial) = self
            .state
            .seats
            .values()
            .find_map(|seat| Some((seat.device.as_ref()?, seat.press?)))
            .ok_or_else(|| {
                anyhow::anyhow!(self.state.language.text(
                    "Release the mouse button and drag the track again",
                    "Отпустите кнопку мыши и перетащите композицию снова"
                ))
            })?;
        let source = manager.create_data_source(&self.queue.handle(), uris);
        source.offer(URI_LIST.into());
        source.set_actions(wl_data_device_manager::DndAction::Copy);
        device.start_drag(Some(&source), &self.origin, None, serial);
        self.state.outgoing = Some(source);
        self.connection.flush()?;
        Ok(())
    }

    pub fn dragging(&self) -> bool {
        self.state.outgoing.is_some()
    }

    pub fn cancel_drag(&mut self) {
        if let Some(source) = self.state.outgoing.take() {
            source.destroy();
        }
        let _ = self.connection.flush();
    }

    pub fn poll(&mut self, playlist: Option<i64>, language: Language) -> Result<Vec<DroppedFiles>> {
        self.state.playlist = playlist;
        self.state.language = language;
        // Winit reads the shared display socket; only dispatch our own pending queue.
        self.queue.dispatch_pending(&mut self.state)?;
        if let Some(transfer) = &self.state.transfer {
            let result = match transfer.receiver.try_recv() {
                Ok(paths) => Some(paths),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(io::Error::other(language.text(
                    "File transfer interrupted. Drop the files again.",
                    "Получение файлов прервано. Повторите перетаскивание.",
                )))),
            };
            if let Some(paths) = result {
                let transfer = self.state.transfer.take().expect("transfer exists");
                if paths.is_ok() && transfer.offer.version() >= 3 {
                    transfer.offer.finish();
                }
                transfer.offer.destroy();
                self.state.received.push(DroppedFiles {
                    playlist: transfer.playlist,
                    paths,
                });
            }
        }
        match self.connection.flush() {
            Ok(()) => {}
            Err(WaylandError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        Ok(std::mem::take(&mut self.state.received))
    }
}

impl Drop for WaylandDrop {
    fn drop(&mut self) {
        self.cancel_drag();
        if let Some(transfer) = self.state.transfer.take() {
            transfer.offer.destroy();
        }
        for (_, offer) in self.state.offers.drain() {
            offer.proxy.destroy();
        }
        for (_, seat) in self.state.seats.drain() {
            seat.release();
        }
        let _ = self.connection.flush();
        // Guest Backend also frees its registry/manager proxies without disconnecting
        // eframe's display. The borrowed origin surface is never destroyed here.
    }
}

#[derive(Default)]
struct State {
    surface: usize,
    manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seats: HashMap<u32, Seat>,
    offers: HashMap<ObjectId, Offer>,
    playlist: Option<i64>,
    language: Language,
    transfer: Option<Transfer>,
    received: Vec<DroppedFiles>,
    outgoing: Option<wl_data_source::WlDataSource>,
}

struct Seat {
    proxy: wl_seat::WlSeat,
    device: Option<wl_data_device::WlDataDevice>,
    dragging: Option<ObjectId>,
    pointer: Option<wl_pointer::WlPointer>,
    focused: bool,
    press: Option<u32>,
}

impl Seat {
    fn release(self) {
        if let Some(pointer) = self.pointer {
            pointer.release();
        }
        if let Some(device) = self.device
            && device.version() >= 2
        {
            device.release();
        }
        if self.proxy.version() >= 5 {
            self.proxy.release();
        }
    }
}

struct Offer {
    proxy: wl_data_offer::WlDataOffer,
    uri_list: bool,
    accepted: bool,
    copy: bool,
}

struct Transfer {
    offer: wl_data_offer::WlDataOffer,
    seat: u32,
    playlist: Option<i64>,
    receiver: Receiver<Paths>,
}

impl State {
    fn bind_devices(&mut self, qh: &QueueHandle<Self>) {
        if let Some(manager) = &self.manager {
            for (&name, seat) in &mut self.seats {
                if seat.device.is_none() {
                    seat.device = Some(manager.get_data_device(&seat.proxy, qh, name));
                }
            }
        }
    }

    fn discard(&mut self, id: ObjectId) {
        if let Some(offer) = self.offers.remove(&id) {
            offer.proxy.destroy();
        }
    }

    fn receive(&mut self, offer: Offer, seat: u32) {
        if !offer.accepted || (offer.proxy.version() >= 3 && !offer.copy) {
            offer.proxy.destroy();
            return;
        }
        let result = (|| -> io::Result<Receiver<Paths>> {
            if self.transfer.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    self.language.text(
                        "Wait for the previous file transfer to finish.",
                        "Дождитесь завершения предыдущего перетаскивания.",
                    ),
                ));
            }
            let (reader, writer) = UnixStream::pair()?;
            let (sender, receiver) = mpsc::channel();
            let language = self.language;
            std::thread::Builder::new()
                .name("almavorn-file-drop".into())
                .spawn(move || {
                    let _ = sender.send(read_paths(reader, language));
                })?;
            offer.proxy.receive(URI_LIST.into(), writer.as_fd());
            Ok(receiver)
        })();
        match result {
            Ok(receiver) => {
                self.transfer = Some(Transfer {
                    offer: offer.proxy,
                    seat,
                    playlist: self.playlist,
                    receiver,
                })
            }
            Err(error) => {
                offer.proxy.destroy();
                self.received.push(DroppedFiles {
                    playlist: self.playlist,
                    paths: Err(error),
                });
            }
        }
    }
}

fn read_paths(mut reader: UnixStream, language: Language) -> Paths {
    let deadline = Instant::now() + Duration::from_secs(15);
    let timed_out = || {
        io::Error::new(
            io::ErrorKind::TimedOut,
            language.text(
                "File transfer timed out. Drop the files again.",
                "Время получения файлов истекло. Повторите перетаскивание.",
            ),
        )
    };
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timed_out());
        }
        reader.set_read_timeout(Some(remaining))?;
        let read = match reader.read(&mut buffer) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Err(timed_out());
            }
            Err(error) => return Err(error),
        };
        if read == 0 {
            break;
        }
        if bytes.len() + read > 8 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                language.text(
                    "Drop fewer files at once.",
                    "Перетащите меньше файлов за один раз.",
                ),
            ));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            language.text(
                "Cannot read the file list. Drop the files again.",
                "Не удалось прочитать список файлов. Повторите перетаскивание.",
            ),
        )
    })?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            url::Url::parse(line)
                .ok()
                .and_then(|url| url.to_file_path().ok())
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        language.text(
                            "Only local files can be dropped.",
                            "Перетаскивайте только локальные файлы.",
                        ),
                    )
                })
        })
        .collect()
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                match interface.as_str() {
                    "wl_data_device_manager" if state.manager.is_none() => {
                        state.manager = Some(registry.bind(name, version.min(3), qh, ()));
                    }
                    "wl_seat" => {
                        let proxy = registry.bind(name, version.min(5), qh, ());
                        state.seats.insert(
                            name,
                            Seat {
                                proxy,
                                device: None,
                                dragging: None,
                                pointer: None,
                                focused: false,
                                press: None,
                            },
                        );
                    }
                    _ => {}
                }
                state.bind_devices(qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(seat) = state.seats.remove(&name) {
                    if state
                        .transfer
                        .as_ref()
                        .is_some_and(|transfer| transfer.seat == name)
                    {
                        let transfer = state.transfer.take().expect("transfer exists");
                        transfer.offer.destroy();
                        state.received.push(DroppedFiles {
                            playlist: transfer.playlist,
                            paths: Err(io::Error::other(state.language.text(
                                "File transfer interrupted. Drop the files again.",
                                "Получение файлов прервано. Повторите перетаскивание.",
                            ))),
                        });
                    }
                    if let Some(id) = seat.dragging.clone() {
                        state.discard(id);
                    }
                    seat.release();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => {
                state.offers.insert(
                    id.id(),
                    Offer {
                        proxy: id,
                        uri_list: false,
                        accepted: false,
                        copy: false,
                    },
                );
            }
            wl_data_device::Event::Enter {
                serial,
                surface,
                id: Some(id),
                ..
            } => {
                let matching =
                    surface.id().as_ptr() as usize == state.surface && state.outgoing.is_none();
                if let Some(offer) = state.offers.get_mut(&id.id()) {
                    offer.accepted = matching && offer.uri_list;
                    id.accept(serial, offer.accepted.then(|| URI_LIST.into()));
                    if offer.accepted && id.version() >= 3 {
                        let copy = wl_data_device_manager::DndAction::Copy;
                        id.set_actions(copy, copy);
                    }
                }
                if let Some(seat) = state.seats.get_mut(name) {
                    seat.dragging = Some(id.id());
                }
            }
            wl_data_device::Event::Leave => {
                if let Some(id) = state
                    .seats
                    .get_mut(name)
                    .and_then(|seat| seat.dragging.take())
                {
                    state.discard(id);
                }
            }
            wl_data_device::Event::Drop => {
                if let Some(id) = state
                    .seats
                    .get_mut(name)
                    .and_then(|seat| seat.dragging.take())
                    && let Some(offer) = state.offers.remove(&id)
                {
                    state.receive(offer, *name);
                }
            }
            wl_data_device::Event::Selection { id: Some(id) } => state.discard(id.id()),
            _ => {}
        }
    }
    event_created_child!(State, wl_data_device::WlDataDevice, [0 => (wl_data_offer::WlDataOffer, ())]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        state: &mut Self,
        offer: &wl_data_offer::WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(data) = state.offers.get_mut(&offer.id()) {
            match event {
                wl_data_offer::Event::Offer { mime_type } if mime_type == URI_LIST => {
                    data.uri_list = true
                }
                wl_data_offer::Event::Action { dnd_action } => {
                    data.copy = dnd_action == WEnum::Value(wl_data_device_manager::DndAction::Copy);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && let Some((&name, seat)) = state
                .seats
                .iter_mut()
                .find(|(_, seat)| seat.proxy == *proxy)
        {
            if capabilities.contains(wl_seat::Capability::Pointer) {
                if seat.pointer.is_none() {
                    seat.pointer = Some(proxy.get_pointer(qh, name));
                }
            } else {
                if let Some(pointer) = seat.pointer.take() {
                    pointer.release();
                }
                seat.press = None;
                seat.focused = false;
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(seat) = state.seats.get_mut(name) else {
            return;
        };
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                seat.focused = surface.id().as_ptr() as usize == state.surface
            }
            wl_pointer::Event::Leave { .. } => {
                seat.focused = false;
                seat.press = None;
            }
            wl_pointer::Event::Button {
                serial,
                button: 0x110,
                state: WEnum::Value(button),
                ..
            } => {
                seat.press =
                    (seat.focused && button == wl_pointer::ButtonState::Pressed).then_some(serial);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_source::WlDataSource, Arc<[u8]>> for State {
    fn event(
        state: &mut Self,
        source: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        uris: &Arc<[u8]>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } if mime_type == URI_LIST => {
                let uris = uris.clone();
                // The target reads a pipe. Keep its I/O off the rendering thread.
                let _ = std::thread::Builder::new()
                    .name("almavorn-file-drag".into())
                    .spawn(move || {
                        let _ = std::fs::File::from(fd).write_all(&uris);
                    });
            }
            wl_data_source::Event::Cancelled | wl_data_source::Event::DndFinished => {
                if state.outgoing.as_ref() == Some(source) {
                    state.outgoing.take();
                    source.destroy();
                }
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_data_device_manager::WlDataDeviceManager);
