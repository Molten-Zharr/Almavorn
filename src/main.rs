use almavorn::{
    app::{App, Options, Target},
    input::{Input, Key, KeyPress},
    ui,
};
use anyhow::Result;
use eframe::egui;
use egui_ratatui::RataguiBackend;
use ratatui::Terminal;
use ratatui::backend::Backend;
mod gui_fonts;
use std::time::{Duration, Instant};

fn main() {
    if let Err(error) = run() {
        eprintln!("Almavorn: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(options) = Options::parse()? else {
        return Ok(());
    };
    let mut app = App::new(&options.directory)?;
    if !options.files.is_empty() {
        app.start_import(options.files)?;
    }
    let mut font_settings = (
        app.settings.appearance.font,
        app.settings.appearance.font_size,
    );
    let backend = gui_fonts::backend(font_settings.0, font_settings.1, 120, 50);
    let mut terminal = Terminal::new(RataguiBackend::new("almavorn", backend))?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([382.0, 420.0]),
        ..Default::default()
    };
    let mut previous_click: Option<(egui::Pos2, Instant)> = None;
    eframe::run_ui_native("Almavorn · GUITUI", options, move |root, _| {
        let ctx = root.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(50));
        app.tick();
        let text_active = app.accepts_text();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(root, |ui| {
                let requested_font = (
                    app.settings.appearance.font,
                    app.settings.appearance.font_size,
                );
                if requested_font != font_settings {
                    let dimensions = terminal
                        .backend()
                        .soft_backend
                        .size()
                        .unwrap_or(ratatui::layout::Size::new(120, 50));
                    terminal.backend_mut().soft_backend = gui_fonts::backend(
                        requested_font.0,
                        requested_font.1,
                        dimensions.width,
                        dimensions.height,
                    );
                    let _ = terminal.clear();
                    font_settings = requested_font;
                }
                // Resize before drawing: rendered hit areas and mouse coordinates must use the same grid.
                let available = ui.available_size();
                let backend = &mut terminal.backend_mut().soft_backend;
                let columns =
                    (available.x / backend.char_width.max(1) as f32).clamp(1.0, 512.0) as u16;
                let rows =
                    (available.y / backend.char_height.max(1) as f32).clamp(1.0, 256.0) as u16;
                if backend.size().ok() != Some(ratatui::layout::Size::new(columns, rows)) {
                    backend.resize(columns, rows);
                }
                if let Err(error) = terminal.draw(|frame| ui::render(&mut app, frame)) {
                    app.notice = format!(
                        "{}: {error}",
                        app.text("Rendering failed", "Ошибка отображения")
                    );
                    app.notice_error = true;
                }
                let backend = terminal.backend_mut();
                let image = backend.to_egui_image();
                let image_size = egui::vec2(image.size[0] as f32, image.size[1] as f32);
                if let Some(texture) = &mut backend.text_handle {
                    texture.set(image, egui::TextureOptions::NEAREST);
                } else {
                    backend.text_handle =
                        Some(ctx.load_texture("almavorn", image, egui::TextureOptions::NEAREST));
                }
                let texture_id = backend
                    .text_handle
                    .as_ref()
                    .expect("Terminal texture exists")
                    .id();
                let response = ui.add(
                    egui::Image::new((texture_id, image_size)).sense(egui::Sense::click_and_drag()),
                );
                response.request_focus();
                let dimensions = terminal.backend().soft_backend.size().ok();
                let Some(dimensions) = dimensions else {
                    return;
                };
                let to_cell = |point: egui::Pos2| -> Option<(u16, u16)> {
                    if !response.rect.contains(point)
                        || response.rect.width() <= 0.0
                        || response.rect.height() <= 0.0
                    {
                        return None;
                    }
                    Some((
                        ((point.x - response.rect.min.x) / response.rect.width()
                            * dimensions.width as f32)
                            .floor() as u16,
                        ((point.y - response.rect.min.y) / response.rect.height()
                            * dimensions.height as f32)
                            .floor() as u16,
                    ))
                };
                let (events, pointer, down, dropped) = ctx.input(|input| {
                    (
                        input.events.clone(),
                        input.pointer.hover_pos(),
                        input.pointer.primary_down(),
                        input
                            .raw
                            .dropped_files
                            .iter()
                            .filter_map(|file| file.path.clone())
                            .collect::<Vec<_>>(),
                    )
                });
                for event in events {
                    match event {
                        egui::Event::Key {
                            key,
                            physical_key,
                            pressed: true,
                            modifiers,
                            ..
                        } => {
                            let physical = physical_key.unwrap_or(key);
                            let key = if key == egui::Key::Plus
                                || (physical == egui::Key::Equals && modifiers.shift)
                            {
                                egui::Key::Plus
                            } else {
                                physical
                            };
                            if let Some(key) = convert_key(key) {
                                app.handle(Input::Key(KeyPress {
                                    key,
                                    ctrl: modifiers.ctrl || modifiers.command,
                                    alt: modifiers.alt,
                                    shift: modifiers.shift,
                                }));
                            }
                        }
                        egui::Event::Text(text) | egui::Event::Paste(text) if text_active => {
                            app.handle(Input::Text(text))
                        }
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            ..
                        } => {
                            if let Some((x, y)) = to_cell(pos) {
                                let double =
                                    previous_click.as_ref().is_some_and(|(point, time)| {
                                        point.distance(pos) < 5.0
                                            && time.elapsed() < Duration::from_millis(400)
                                    });
                                previous_click = Some((pos, Instant::now()));
                                app.handle(Input::Click { x, y, double });
                            }
                        }
                        egui::Event::PointerMoved(pos) => {
                            if let Some((x, y)) = to_cell(pos) {
                                app.handle(Input::Move { x, y });
                                if down
                                    && app.hits.iter().any(|hit| {
                                        matches!(hit.target, Target::Seek(_))
                                            && hit
                                                .area
                                                .contains(ratatui::layout::Position::new(x, y))
                                    })
                                {
                                    app.handle(Input::Click {
                                        x,
                                        y,
                                        double: false,
                                    });
                                }
                            }
                        }
                        egui::Event::MouseWheel { delta, .. } => {
                            if let Some((x, y)) = pointer.and_then(to_cell)
                                && delta.y != 0.0
                            {
                                app.handle(Input::Scroll {
                                    x,
                                    y,
                                    delta: if delta.y > 0.0 { -3 } else { 3 },
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if !dropped.is_empty()
                    && let Err(error) = app.start_import(dropped)
                {
                    app.notice = format!(
                        "{}: {error}",
                        app.text("Import failed", "Ошибка добавления")
                    );
                    app.notice_error = true;
                }
            });
        if app.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    })
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn convert_key(key: egui::Key) -> Option<Key> {
    Some(match key {
        egui::Key::ArrowUp => Key::Up,
        egui::Key::ArrowDown => Key::Down,
        egui::Key::ArrowLeft => Key::Left,
        egui::Key::ArrowRight => Key::Right,
        egui::Key::Escape => Key::Escape,
        egui::Key::Tab => Key::Tab,
        egui::Key::Backspace => Key::Backspace,
        egui::Key::Delete => Key::Delete,
        egui::Key::Insert => Key::Insert,
        egui::Key::Enter => Key::Enter,
        egui::Key::Space => Key::Char(' '),
        egui::Key::Home => Key::Home,
        egui::Key::End => Key::End,
        egui::Key::PageUp => Key::PageUp,
        egui::Key::PageDown => Key::PageDown,
        egui::Key::F1 => Key::F(1),
        egui::Key::F2 => Key::F(2),
        egui::Key::F3 => Key::F(3),
        egui::Key::F4 => Key::F(4),
        egui::Key::F5 => Key::F(5),
        egui::Key::F6 => Key::F(6),
        egui::Key::F7 => Key::F(7),
        egui::Key::F8 => Key::F(8),
        egui::Key::F9 => Key::F(9),
        egui::Key::F10 => Key::F(10),
        egui::Key::F11 => Key::F(11),
        egui::Key::F12 => Key::F(12),
        egui::Key::Plus => Key::Char('+'),
        egui::Key::Equals => Key::Char('='),
        egui::Key::Minus => Key::Char('-'),
        egui::Key::Slash => Key::Char('/'),
        other => {
            let name = other.name();
            if name.chars().count() == 1 {
                Key::Char(name.chars().next()?.to_ascii_lowercase())
            } else {
                return None;
            }
        }
    })
}
