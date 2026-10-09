use almavorn::{
    app::{App, Options, Target},
    input::{Input, Key, KeyPress},
    ui,
};
use anyhow::Result;
use eframe::egui::{self, emath::GuiRounding};
use egui_ratatui::RataguiBackend;
use ratatui::Terminal;
use ratatui::backend::Backend;
mod gui_fonts;
mod gui_renderer;
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
    app.view.graphical_keycaps = true;
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
    let mut image_cache = gui_renderer::ImageCache::default();
    let mut playback_painter = gui_renderer::PlaybackPainter::default();
    eframe::run_ui_native("Almavorn · GUITUI", options, move |root, _| {
        let ctx = root.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(16));
        app.tick();
        let text_active = app.accepts_text();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show_inside(root, |ui| {
                let pixels_per_point = ctx.pixels_per_point();
                let requested_font = (
                    app.settings.appearance.font,
                    (f32::from(app.settings.appearance.font_size) * pixels_per_point)
                        .round()
                        .clamp(1.0, f32::from(u16::MAX)) as u16,
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
                    image_cache.invalidate();
                    font_settings = requested_font;
                }
                // Resize before drawing: rendered hit areas and mouse coordinates must use the same grid.
                let available = ui.available_size();
                let backend = &mut terminal.backend_mut().soft_backend;
                let columns = (available.x * pixels_per_point / backend.char_width.max(1) as f32)
                    .clamp(1.0, f32::from(u16::MAX)) as u16;
                let rows = (available.y * pixels_per_point / backend.char_height.max(1) as f32)
                    .clamp(1.0, f32::from(u16::MAX)) as u16;
                if backend.size().ok() != Some(ratatui::layout::Size::new(columns, rows)) {
                    gui_renderer::resize(backend, columns, rows);
                }
                // Only real pointer events may take selection back from keys or the wheel.
                // Process them before painting so hover selection is visible this frame.
                let image_rect = egui::Rect::from_min_size(
                    ui.next_widget_position().round_to_pixels(pixels_per_point),
                    egui::vec2(
                        (backend.char_width * usize::from(columns)) as f32,
                        (backend.char_height * usize::from(rows)) as f32,
                    ) / pixels_per_point,
                );
                if let Some(movement) = pointer_movement(&ctx, image_rect, columns, rows) {
                    app.handle(movement);
                }
                if let Err(error) = terminal.draw(|frame| ui::render(&mut app, frame)) {
                    app.view.notice = format!(
                        "{}: {error}",
                        app.text("Rendering failed", "Ошибка отображения")
                    );
                    app.view.notice_error = true;
                }
                let backend = terminal.backend_mut();
                let image_size = image_cache.update(&ctx, backend);
                let texture_id = backend
                    .text_handle
                    .as_ref()
                    .expect("Terminal texture exists")
                    .id();
                let (canvas, response) =
                    ui.allocate_exact_size(available, egui::Sense::click_and_drag());
                let image_rect = egui::Rect::from_min_size(
                    canvas.min.round_to_pixels(pixels_per_point),
                    image_size / pixels_per_point,
                );
                let [r, g, b] = app.settings.current_palette().color("background");
                // Keep physical font pixels 1:1. Only the sub-cell remainder is background.
                ui.painter()
                    .rect_filled(canvas, 0, egui::Color32::from_rgb(r, g, b));
                ui.painter().image(
                    texture_id,
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
                response.request_focus();
                let dimensions = terminal.backend().soft_backend.size().ok();
                let Some(dimensions) = dimensions else {
                    return;
                };
                if let Some(point) = ctx.input(|input| input.pointer.hover_pos()) {
                    let x = ((point.x - image_rect.min.x) / image_rect.width()
                        * f32::from(dimensions.width))
                    .floor() as u16;
                    let y = ((point.y - image_rect.min.y) / image_rect.height()
                        * f32::from(dimensions.height))
                    .floor() as u16;
                    let target = app.target_at(ratatui::layout::Position::new(x, y));
                    let cursor = match target {
                        _ if matches!(
                            app.view.workspace.gesture,
                            Some(almavorn::workspace::Gesture::Playlist { .. })
                        ) =>
                        {
                            egui::CursorIcon::Grabbing
                        }
                        Some(Target::Playlist(id)) if app.playlist_movable(*id) => {
                            egui::CursorIcon::Grab
                        }
                        Some(
                            Target::PlaybackVolume(_)
                            | Target::Seek(_)
                            | Target::SettingsVolume(_, _),
                        ) => egui::CursorIcon::ResizeHorizontal,
                        Some(Target::EqualizerGain { vertical: true, .. }) => {
                            egui::CursorIcon::ResizeVertical
                        }
                        Some(Target::EqualizerGain { .. }) => egui::CursorIcon::ResizeHorizontal,
                        Some(Target::TunerSlider(_, _)) => egui::CursorIcon::ResizeHorizontal,
                        Some(
                            Target::SortColumn(_)
                            | Target::Action(_)
                            | Target::Mode(_)
                            | Target::CommandMenu(_)
                            | Target::CommandRow(_)
                            | Target::ManagePanels
                            | Target::PanelCollapse(_)
                            | Target::PanelClose(_)
                            | Target::Setting(_)
                            | Target::SettingSelect(_)
                            | Target::SettingAdjust(_, _)
                            | Target::SettingHelp(_)
                            | Target::SettingsPage(_)
                            | Target::SettingsTab(_)
                            | Target::CloseDialog
                            | Target::SearchResult(_)
                            | Target::SearchPlay
                            | Target::SearchPlaylist(_)
                            | Target::SearchAll(_)
                            | Target::QueryKeyboard
                            | Target::ClearFilter
                            | Target::EqualizerToggle
                            | Target::EqualizerPresets
                            | Target::EqualizerPreset(_)
                            | Target::EqualizerSelect(_)
                            | Target::EqualizerReset
                            | Target::TunerReverse
                            | Target::TunerReset
                            | Target::TunerSelect(_)
                            | Target::SettingsTimeline(_, _),
                        ) => egui::CursorIcon::PointingHand,
                        Some(Target::SearchInput | Target::FilterInput) => egui::CursorIcon::Text,
                        Some(Target::PanelMove(_)) => egui::CursorIcon::Grab,
                        Some(Target::PanelResize(index)) => {
                            match app.view.workspace.splits[*index].axis {
                                almavorn::workspace::Axis::Horizontal => {
                                    egui::CursorIcon::ResizeHorizontal
                                }
                                almavorn::workspace::Axis::Vertical => {
                                    egui::CursorIcon::ResizeVertical
                                }
                            }
                        }
                        _ => egui::CursorIcon::Default,
                    };
                    ui.ctx().set_cursor_icon(cursor);
                }
                let to_cell = |point| pointer_cell(image_rect, dimensions, point);
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
                    if handle_pointer_event(
                        &mut app,
                        image_rect,
                        dimensions,
                        &event,
                        down,
                        &mut previous_click,
                    ) {
                        continue;
                    }
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
                        egui::Event::MouseWheel { delta, .. } => {
                            if let Some((x, y)) = pointer.and_then(to_cell)
                                && delta.y != 0.0
                            {
                                app.handle(Input::Scroll {
                                    x,
                                    y,
                                    delta: if delta.y > 0.0 { -1 } else { 1 },
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if app.view.workspace.gesture.is_some() && !down {
                    if let Some(point) = pointer {
                        release_pointer(&mut app, image_rect, dimensions, point);
                    } else {
                        app.handle(Input::CancelPointer);
                    }
                }
                if !dropped.is_empty()
                    && let Err(error) = app.start_import(dropped)
                {
                    app.view.notice = format!(
                        "{}: {error}",
                        app.text("Import failed", "Ошибка добавления")
                    );
                    app.view.notice_error = true;
                }
                playback_painter.paint(ui, image_rect, &app, dimensions);
                let overlay = app.view.overlay_area;
                if !overlay.is_empty() {
                    let area = overlay;
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(
                            f32::from(area.x) / f32::from(dimensions.width),
                            f32::from(area.y) / f32::from(dimensions.height),
                        ),
                        egui::pos2(
                            f32::from(area.right()) / f32::from(dimensions.width),
                            f32::from(area.bottom()) / f32::from(dimensions.height),
                        ),
                    );
                    let popup = egui::Rect::from_min_max(
                        image_rect.min + uv.min.to_vec2() * image_rect.size(),
                        image_rect.min + uv.max.to_vec2() * image_rect.size(),
                    );
                    ui.painter()
                        .image(texture_id, popup, uv, egui::Color32::WHITE);
                }
                gui_renderer::paint_keycaps(ui, image_rect, &app, dimensions);
            });
        if app.view.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    })
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn pointer_cell(
    image: egui::Rect,
    size: ratatui::layout::Size,
    point: egui::Pos2,
) -> Option<(u16, u16)> {
    if !image.contains(point) || image.width() <= 0.0 || image.height() <= 0.0 {
        return None;
    }
    Some((
        ((point.x - image.left()) / image.width() * f32::from(size.width)).floor() as u16,
        ((point.y - image.top()) / image.height() * f32::from(size.height)).floor() as u16,
    ))
}

fn handle_pointer_event(
    app: &mut App,
    image: egui::Rect,
    size: ratatui::layout::Size,
    event: &egui::Event,
    down: bool,
    previous_click: &mut Option<(egui::Pos2, Instant)>,
) -> bool {
    match *event {
        egui::Event::PointerButton {
            pos,
            button,
            pressed: true,
            ..
        } if matches!(
            button,
            egui::PointerButton::Primary | egui::PointerButton::Secondary
        ) =>
        {
            if let Some((x, y)) = pointer_cell(image, size, pos) {
                if button == egui::PointerButton::Secondary {
                    app.handle(Input::SecondaryClick { x, y });
                } else {
                    let double = previous_click.as_ref().is_some_and(|(point, time)| {
                        point.distance(pos) < 5.0 && time.elapsed() < Duration::from_millis(400)
                    });
                    *previous_click = Some((pos, Instant::now()));
                    if !slider_pointer(app, image, size, pos, true, false) {
                        app.handle(Input::Click { x, y, double });
                        if let Some(ratio) = seek_ratio(app, image, size, pos) {
                            app.scrub(ratio, false);
                        }
                    }
                }
            }
        }
        egui::Event::PointerMoved(pos) => {
            if down && !slider_pointer(app, image, size, pos, false, false) {
                if let Some(ratio) = seek_ratio(app, image, size, pos) {
                    app.scrub(ratio, false);
                } else if let Some((x, y)) = pointer_cell(image, size, pos) {
                    app.handle(Input::Drag { x, y });
                }
            }
        }
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            ..
        } => {
            release_pointer(app, image, size, pos);
        }
        _ => return false,
    }
    true
}

fn release_pointer(
    app: &mut App,
    image: egui::Rect,
    size: ratatui::layout::Size,
    point: egui::Pos2,
) {
    if slider_pointer(app, image, size, point, false, true) {
        return;
    }
    if let Some(ratio) = seek_ratio(app, image, size, point) {
        app.scrub(ratio, true);
        return;
    }
    let point = point.clamp(image.min, image.max - egui::vec2(0.1, 0.1));
    if let Some((x, y)) = pointer_cell(image, size, point) {
        app.handle(Input::Release { x, y });
    }
}

fn slider_pointer(
    app: &mut App,
    image: egui::Rect,
    size: ratatui::layout::Size,
    point: egui::Pos2,
    begin: bool,
    release: bool,
) -> bool {
    use almavorn::workspace::Gesture;
    use ratatui::layout::Position;

    if size.width == 0 || size.height == 0 || image.width() <= 0.0 || image.height() <= 0.0 {
        return false;
    }
    let cell_width = image.width() / f32::from(size.width);
    let target = match app.view.workspace.gesture {
        Some(Gesture::TunerSlider(index, area)) => Target::TunerSlider(index, area),
        Some(Gesture::PlaybackVolume(area)) => Target::PlaybackVolume(area),
        Some(Gesture::Volume(index, area)) => Target::SettingsVolume(index, area),
        None if begin && image.contains(point) => {
            let position = Position::new(
                ((point.x - image.left()) / cell_width).floor() as u16,
                ((point.y - image.top()) / image.height() * f32::from(size.height)).floor() as u16,
            );
            let Some(
                target @ (Target::TunerSlider(_, _)
                | Target::PlaybackVolume(_)
                | Target::SettingsVolume(_, _)),
            ) = app.target_at(position)
            else {
                return false;
            };
            target.clone()
        }
        _ => return false,
    };
    let area = match target {
        Target::TunerSlider(_, area)
        | Target::PlaybackVolume(area)
        | Target::SettingsVolume(_, area) => area,
        _ => return false,
    };
    // Retain the fractional position between the centers of the endpoint cells.
    let left = image.left() + (f32::from(area.x) + 0.5) * cell_width;
    let width = (f32::from(area.width.saturating_sub(1)) * cell_width).max(1.0);
    let ratio = f64::from((point.x - left) / width);
    if let Target::TunerSlider(index, _) = target {
        app.drag_tuner(index, area, ratio, release);
    } else {
        app.drag_volume(target, ratio, release);
    }
    // Opening a dialog while holding the mouse may make the old slider inactive.
    // A consumed release must still end its pointer capture.
    if release {
        app.view.workspace.gesture = None;
    }
    true
}

fn seek_ratio(
    app: &App,
    image: egui::Rect,
    size: ratatui::layout::Size,
    point: egui::Pos2,
) -> Option<f64> {
    let Some(almavorn::workspace::Gesture::Seek(area)) = &app.view.workspace.gesture else {
        return None;
    };
    let cell_width = image.width() / f32::from(size.width);
    let left = image.left() + f32::from(area.x) * cell_width;
    let width = (f32::from(area.width) * cell_width - 1.0).max(1.0);
    Some(f64::from(((point.x - left) / width).clamp(0.0, 1.0)))
}

fn pointer_movement(
    ctx: &egui::Context,
    area: egui::Rect,
    columns: u16,
    rows: u16,
) -> Option<Input> {
    ctx.input(|input| {
        if !input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::PointerMoved(_) | egui::Event::PointerGone
            )
        }) {
            return None;
        }
        let (x, y) = input
            .pointer
            .hover_pos()
            .filter(|point| area.width() > 0.0 && area.height() > 0.0 && area.contains(*point))
            .map(|point| {
                (
                    ((point.x - area.min.x) / area.width() * f32::from(columns)).floor() as u16,
                    ((point.y - area.min.y) / area.height() * f32::from(rows)).floor() as u16,
                )
            })
            .unwrap_or((u16::MAX, u16::MAX));
        Some(Input::Move { x, y })
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_volume_release_ends_capture_after_opening_settings() {
        use almavorn::app::Dialog;
        use ratatui::{backend::TestBackend, layout::Size};

        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        let size = Size::new(120, 50);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
        let bar = app
            .view
            .hits
            .iter()
            .find_map(|hit| match hit.target {
                Target::PlaybackVolume(area) => Some(area),
                _ => None,
            })
            .unwrap();
        let image = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1080.0, 900.0));
        let pos = egui::pos2(
            f32::from(bar.x + bar.width / 2) * 9.0,
            f32::from(bar.y) * 18.0,
        );
        let mut previous_click = None;
        let event = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        assert!(handle_pointer_event(
            &mut app,
            image,
            size,
            &event(true),
            true,
            &mut previous_click
        ));
        assert!(app.view.workspace.gesture.is_some());
        app.handle(Input::Key(KeyPress::plain(Key::F(2))));
        assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
        assert!(handle_pointer_event(
            &mut app,
            image,
            size,
            &event(false),
            false,
            &mut previous_click
        ));
        assert!(app.view.workspace.gesture.is_none());
    }

    #[test]
    fn gui_volume_drags_reach_every_percent_through_pointer_events() {
        use almavorn::app::SettingsPage;
        use ratatui::{backend::TestBackend, layout::Size};

        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        app.view.graphical_keycaps = true;
        for (columns, rows) in [(120, 50), (70, 30)] {
            for settings in [false, true] {
                app.view.dialog = None;
                if settings {
                    app.open_settings_page(SettingsPage::General);
                }
                let mut terminal = Terminal::new(TestBackend::new(columns, rows)).unwrap();
                terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
                let bar = app
                    .view
                    .hits
                    .iter()
                    .find_map(|hit| match hit.target {
                        Target::PlaybackVolume(area) if !settings => Some(area),
                        Target::SettingsVolume(_, area) if settings => Some(area),
                        _ => None,
                    })
                    .unwrap();
                let size = Size::new(columns, rows);
                let image = egui::Rect::from_min_size(
                    egui::pos2(37.25, 61.75),
                    egui::vec2(f32::from(columns) * 9.0, f32::from(rows) * 18.0),
                );
                let left = image.left() + (f32::from(bar.x) + 0.5) * 9.0;
                let span = f32::from(bar.width - 1) * 9.0;
                let y = image.top() + (f32::from(bar.y) + 0.5) * 18.0;
                let point = |percent: u16| egui::pos2(left + f32::from(percent) / 100.0 * span, y);
                let button = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                let ctx = egui::Context::default();
                let mut previous_click = None;
                let mut frame = |app: &mut App, events| {
                    let _ = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(image),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            if let Some(movement) = pointer_movement(ui.ctx(), image, columns, rows)
                            {
                                app.handle(movement);
                            }
                            let (events, down) = ui.ctx().input(|input| {
                                (input.events.clone(), input.pointer.primary_down())
                            });
                            for event in events {
                                assert!(handle_pointer_event(
                                    app,
                                    image,
                                    size,
                                    &event,
                                    down,
                                    &mut previous_click
                                ));
                            }
                        },
                    );
                };
                frame(
                    &mut app,
                    vec![egui::Event::PointerMoved(point(0)), button(point(0), true)],
                );
                assert!(app.view.workspace.gesture.is_some());
                for percent in (0..=100).chain((0..100).rev()) {
                    let mut position = point(percent);
                    position.y += 40.0;
                    frame(&mut app, vec![egui::Event::PointerMoved(position)]);
                    assert_eq!(
                        (app.settings.volume * 100.0).round() as u16,
                        percent,
                        "settings={settings}, columns={columns}"
                    );
                }
                frame(&mut app, vec![button(point(51), false)]);
                assert_eq!((app.settings.volume * 100.0).round() as u16, 51);
                assert!(app.view.workspace.gesture.is_none());
                assert!(!app.view.notice_error, "{}", app.view.notice);
                // The frame fallback must not turn a precise value into a cell value.
                frame(&mut app, vec![button(point(50), true)]);
                release_pointer(&mut app, image, size, point(51));
                assert_eq!((app.settings.volume * 100.0).round() as u16, 51);
                assert!(app.view.workspace.gesture.is_none());
                frame(&mut app, vec![button(point(51), false)]);
            }
        }
        drop(app);
        let app = App::new(directory.path()).unwrap();
        assert_eq!((app.settings.volume * 100.0).round() as u16, 51);
    }

    #[test]
    fn gui_tuner_drag_reaches_every_percent_and_preserves_precision_on_release() {
        use almavorn::{app::Dialog, input::Action, tuner::TunerSettings};
        use ratatui::{backend::TestBackend, layout::Size};

        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        app.view.graphical_keycaps = true;
        for (columns, rows, cell_width) in [(120, 50, 9.0), (70, 30, 12.5), (32, 26, 6.0)] {
            let size = Size::new(columns, rows);
            let image = egui::Rect::from_min_size(
                egui::pos2(37.25, 61.75),
                egui::vec2(f32::from(columns) * cell_width, f32::from(rows) * 18.0),
            );
            app.action(Action::PlaybackTuner).unwrap();
            for index in 0..4 {
                let mut terminal = Terminal::new(TestBackend::new(columns, rows)).unwrap();
                terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
                let bar = app
                    .view
                    .hits
                    .iter()
                    .find_map(|hit| match hit.target {
                        Target::TunerSlider(value, area) if value == index => Some(area),
                        _ => None,
                    })
                    .unwrap();
                let minimum = TunerSettings::minimum(index);
                let maximum = TunerSettings::maximum(index);
                let left = image.left() + (f32::from(bar.x) + 0.5) * cell_width;
                let span = f32::from(bar.width - 1) * cell_width;
                let y = image.top() + (f32::from(bar.y) + 0.5) * 18.0;
                let point = |value| {
                    egui::pos2(
                        left + f32::from(value - minimum) / f32::from(maximum - minimum) * span,
                        y,
                    )
                };
                assert!(!slider_pointer(
                    &mut app,
                    image,
                    size,
                    point(minimum),
                    false,
                    false
                ));
                assert!(slider_pointer(
                    &mut app,
                    image,
                    size,
                    point(minimum),
                    true,
                    false
                ));
                for value in (minimum..=maximum).chain((minimum..maximum).rev()) {
                    // Most adjacent values share a cell; keep their fractional positions.
                    let mut position = point(value);
                    position.y += 40.0;
                    assert!(slider_pointer(
                        &mut app, image, size, position, false, false
                    ));
                    assert_eq!(
                        app.settings.tuner.value(index),
                        Some(value),
                        "{columns} cols"
                    );
                }
                for (x, expected) in [
                    (image.left() - 100.0, minimum),
                    (image.right() + 100.0, maximum),
                ] {
                    assert!(slider_pointer(
                        &mut app,
                        image,
                        size,
                        egui::pos2(x, y),
                        false,
                        false
                    ));
                    assert_eq!(app.settings.tuner.value(index), Some(expected));
                }
                let value = if index == 3 { 50 } else { 100 };
                assert!(slider_pointer(
                    &mut app,
                    image,
                    size,
                    point(value),
                    false,
                    true
                ));
                assert_eq!(app.settings.tuner.value(index), Some(value));
                assert!(app.view.workspace.gesture.is_none());
                assert!(!app.view.notice_error, "{}", app.view.notice);
                if index == 3 {
                    assert!(slider_pointer(
                        &mut app,
                        image,
                        size,
                        point(value),
                        true,
                        false,
                    ));
                    app.handle(Input::Key(KeyPress::plain(Key::Escape)));
                    assert!(app.view.workspace.gesture.is_none());
                    assert!(!slider_pointer(
                        &mut app,
                        image,
                        size,
                        point(value + 1),
                        false,
                        false,
                    ));
                    assert_eq!(app.settings.tuner.value(index), Some(value));
                }
                app.handle(Input::Key(KeyPress::plain(Key::Tab)));
            }
            app.handle(Input::Key(KeyPress::plain(Key::Escape)));
            assert!(app.view.dialog.is_none());
            assert!(!slider_pointer(
                &mut app,
                image,
                size,
                image.center(),
                false,
                false
            ));
        }
        assert!(!matches!(
            app.view.dialog,
            Some(Dialog::PlaybackTuner { .. })
        ));
        let expected = app.settings.tuner.clone();
        drop(app);
        let app = App::new(directory.path()).unwrap();
        assert_eq!(app.settings.tuner, expected);
    }

    #[test]
    fn gui_repaints_keys_and_wheel_do_not_reemit_stationary_pointer_movement() {
        let ctx = egui::Context::default();
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 200.0));
        let frame = |events| {
            let mut movement = None;
            let _ = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(area),
                    events,
                    ..Default::default()
                },
                |ui| movement = pointer_movement(ui.ctx(), area, 10, 20),
            );
            movement
        };
        let position = egui::pos2(25.0, 55.0);
        assert!(matches!(
            frame(vec![egui::Event::PointerMoved(position)]),
            Some(Input::Move { x: 2, y: 5 })
        ));
        assert!(
            frame(vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -1.0),
                phase: egui::TouchPhase::Move,
                modifiers: Default::default(),
            }])
            .is_none()
        );
        assert_eq!(ctx.input(|input| input.pointer.hover_pos()), Some(position));
        for _ in 0..5 {
            assert!(frame(vec![]).is_none());
        }
        assert!(
            frame(vec![egui::Event::Key {
                key: egui::Key::ArrowDown,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }])
            .is_none()
        );
        assert!(matches!(
            frame(vec![egui::Event::PointerMoved(egui::pos2(26.0, 55.0))]),
            Some(Input::Move { x: 2, y: 5 })
        ));
        assert!(matches!(
            frame(vec![egui::Event::PointerGone]),
            Some(Input::Move {
                x: u16::MAX,
                y: u16::MAX
            })
        ));
    }
}
