use crate::gui_file_drop::WaylandDrop;
use almavorn::{app::App, input::Input};
use anyhow::{Result, ensure};
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};

struct Transfer {
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<Result<()>>,
}

#[derive(Default)]
pub struct FileDrag {
    origin: Option<u32>,
    pending: Option<(egui::Pos2, Vec<PathBuf>)>,
    transfer: Option<Transfer>,
    was_active: bool,
}

impl Drop for FileDrag {
    fn drop(&mut self) {
        if let Some(transfer) = &self.transfer {
            transfer.cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl FileDrag {
    pub fn new(frame: &impl HasWindowHandle) -> Self {
        let origin = frame
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                RawWindowHandle::Xlib(window) => u32::try_from(window.window).ok(),
                RawWindowHandle::Xcb(window) => Some(window.window.get()),
                _ => None,
            });
        Self {
            origin,
            pending: None,
            transfer: None,
            was_active: false,
        }
    }

    pub fn active(&self, wayland: &Option<WaylandDrop>) -> bool {
        self.transfer.is_some() || wayland.as_ref().is_some_and(WaylandDrop::dragging)
    }

    pub fn poll(&mut self, app: &mut App, ctx: &egui::Context, wayland: &Option<WaylandDrop>) {
        if let Some(transfer) = &self.transfer {
            let result = match transfer.result.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(anyhow::anyhow!("File drag interrupted")))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                self.transfer = None;
                if let Err(error) = result {
                    failure(app, error);
                }
            }
        }
        let active = self.active(wayland);
        if self.was_active && !active {
            ctx.input_mut(|input| input.pointer = Default::default());
            app.handle(Input::CancelPointer);
        }
        self.was_active = active;
        if active {
            ctx.set_cursor_icon(egui::CursorIcon::Copy);
        }
    }

    pub fn event(
        &mut self,
        event: &egui::Event,
        app: &mut App,
        wayland: &mut Option<WaylandDrop>,
        to_cell: impl Fn(egui::Pos2) -> Option<(u16, u16)>,
        down: bool,
        previous_click: &mut Option<(egui::Pos2, Instant)>,
    ) -> bool {
        if self.active(wayland) {
            // Focus can change normally when the destination receives the drop.
            if matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                }
            ) {
                if let Some(wayland) = wayland {
                    wayland.cancel_drag();
                }
                if let Some(transfer) = &self.transfer {
                    transfer.cancel.store(true, Ordering::Relaxed);
                }
            }
            return true;
        }
        if app.view.dialog.is_some() || app.view.layout_editing {
            self.pending = None;
        }
        match event {
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers,
            } => {
                self.pending = None;
                if !modifiers.ctrl
                    && !modifiers.alt
                    && !modifiers.command
                    && (wayland.is_some() || self.origin.is_some())
                    && let Some((x, y)) = to_cell(*pos)
                {
                    let paths = app.drag_file_paths_at(x, y);
                    if !paths.is_empty() {
                        self.pending = Some((*pos, paths));
                    }
                }
            }
            egui::Event::PointerMoved(pos) if down => {
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|(start, _)| start.distance(*pos) >= 8.0)
                {
                    let (_, paths) = self.pending.take().expect("pending drag");
                    *previous_click = None;
                    app.handle(Input::CancelPointer);
                    match uri_list(paths, app.settings.preferences.language)
                        .and_then(|uris| self.start(uris, wayland))
                    {
                        Ok(()) => self.was_active = true,
                        Err(error) => failure(app, error),
                    }
                    return true;
                }
            }
            egui::Event::PointerButton { .. }
            | egui::Event::WindowFocused(false)
            | egui::Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                ..
            } => self.pending = None,
            _ => {}
        }
        false
    }

    fn start(&mut self, uris: Arc<[u8]>, wayland: &mut Option<WaylandDrop>) -> Result<()> {
        if let Some(wayland) = wayland {
            return wayland.start_drag(uris);
        }
        let origin = self
            .origin
            .ok_or_else(|| anyhow::anyhow!("File dragging is unavailable"))?;
        let (sender, result) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("almavorn-x11-drag".into())
            .spawn(move || {
                let _ = sender.send(crate::gui_file_drag_x11::run(origin, uris, worker_cancel));
            })?;
        self.transfer = Some(Transfer { cancel, result });
        Ok(())
    }
}

fn uri_list(paths: Vec<PathBuf>, language: almavorn::model::Language) -> Result<Arc<[u8]>> {
    let mut uris = String::new();
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        ensure!(
            path.is_file(),
            "{}: {}",
            language.text("Audio file not found", "Аудиофайл не найден"),
            path.display()
        );
        let path = path.canonicalize()?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let uri =
            url::Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;
        uris.push_str(uri.as_str());
        uris.push_str("\r\n");
        ensure!(
            uris.len() <= 8 * 1024 * 1024,
            "{}",
            language.text(
                "Drag fewer tracks at once",
                "Перетащите меньше композиций за один раз"
            )
        );
    }
    ensure!(!uris.is_empty(), "No local audio files selected");
    Ok(uris.into_bytes().into())
}

fn failure(app: &mut App, error: impl std::fmt::Display) {
    app.file_drag_failed(error);
}
