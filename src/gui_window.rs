//! Linux client decorations using the desktop's exported GTK color roles.
//! No GTK settings are written; the compositor still owns move/resize operations.
use eframe::egui::{
    self, Color32, CursorIcon, Id, PointerButton, Rect, ResizeDirection, Sense, Stroke,
    ViewportCommand,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
struct Colors {
    background: Color32,
    foreground: Color32,
    backdrop: Color32,
    accent: Color32,
    on_accent: Color32,
}

pub struct WindowFrame {
    colors: Option<Colors>,
    next_refresh: Instant,
    minimize_supported: bool,
}

impl WindowFrame {
    pub fn new() -> Self {
        let niri = std::env::var_os("NIRI_SOCKET").is_some()
            || std::env::var("XDG_CURRENT_DESKTOP")
                .unwrap_or_default()
                .split(':')
                .any(|part| part.eq_ignore_ascii_case("niri"));
        Self {
            colors: desktop_colors(),
            next_refresh: Instant::now() + Duration::from_secs(2),
            minimize_supported: !niri,
        }
    }

    pub fn enabled(&self) -> bool {
        self.colors.is_some()
    }

    pub fn show(&mut self, ui: &mut egui::Ui, close_allowed: bool, russian: bool) -> (Rect, bool) {
        if Instant::now() >= self.next_refresh {
            self.next_refresh = Instant::now() + Duration::from_secs(2);
            if let Some(colors) = desktop_colors() {
                if self.colors.is_none() {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Decorations(false));
                }
                self.colors = Some(colors);
            }
        }
        let outer = ui.max_rect();
        let Some(colors) = self.colors else {
            return (outer, false);
        };
        let (fullscreen, maximized, focused) = ui.input(|input| {
            (
                input.viewport().fullscreen.unwrap_or(false),
                input.viewport().maximized.unwrap_or(false),
                input.focused,
            )
        });
        if fullscreen {
            return (outer, false);
        }
        let edge = 5.0;
        let inner = outer.shrink(edge);
        let title = Rect::from_min_size(inner.min, egui::vec2(inner.width(), 32.0));
        let content = Rect::from_min_max(egui::pos2(inner.left(), title.bottom()), inner.max);
        let background = if focused {
            colors.background
        } else {
            colors.backdrop
        };
        let foreground = legible(colors.foreground, background);
        ui.painter().rect_filled(outer, 0, background);
        ui.painter().rect_stroke(
            outer.shrink(0.5),
            0,
            Stroke::new(
                1.0_f32,
                if focused {
                    colors.accent
                } else {
                    foreground.gamma_multiply(0.35)
                },
            ),
            egui::StrokeKind::Inside,
        );
        ui.painter().line_segment(
            [title.left_bottom(), title.right_bottom()],
            Stroke::new(1.0_f32, colors.accent),
        );

        let mut right = title.right();
        let mut keyboard_focus = false;
        for (kind, enabled, label) in [
            (0, close_allowed, if russian { "Закрыть" } else { "Close" }),
            (
                1,
                true,
                if maximized {
                    if russian {
                        "Восстановить размер"
                    } else {
                        "Restore"
                    }
                } else if russian {
                    "Развернуть"
                } else {
                    "Maximize"
                },
            ),
            (
                2,
                self.minimize_supported,
                if russian {
                    "Свернуть"
                } else {
                    "Minimize"
                },
            ),
        ] {
            // Niri does not implement the xdg-shell minimize request.
            if kind == 2 && !self.minimize_supported {
                continue;
            }
            let rect = Rect::from_min_max(
                egui::pos2(right - 38.0, title.top() + 2.0),
                egui::pos2(right - 2.0, title.bottom() - 2.0),
            );
            right -= 40.0;
            let mut response = ui.interact(
                rect,
                Id::new(("window-control", kind)),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
            keyboard_focus |= response.has_focus();
            let active = enabled
                && (response.hovered()
                    || response.has_focus()
                    || response.is_pointer_button_down_on());
            let fill = if active { colors.accent } else { background };
            let ink = if active {
                legible(colors.on_accent, fill)
            } else {
                foreground
            };
            let ink = if enabled {
                ink
            } else {
                ink.gamma_multiply(0.45)
            };
            ui.painter().rect_filled(rect, 3, fill);
            if response.has_focus() {
                ui.painter().rect_stroke(
                    rect.shrink(1.0),
                    3,
                    Stroke::new(1.0_f32, ink),
                    egui::StrokeKind::Inside,
                );
            }
            let icon = Rect::from_center_size(rect.center(), egui::vec2(12.0, 12.0));
            let stroke = Stroke::new(1.5_f32, ink);
            match kind {
                0 => {
                    ui.painter()
                        .line_segment([icon.left_top(), icon.right_bottom()], stroke);
                    ui.painter()
                        .line_segment([icon.right_top(), icon.left_bottom()], stroke);
                }
                1 if maximized => {
                    ui.painter().rect_stroke(
                        icon.translate(egui::vec2(2.0, -2.0)).shrink(1.0),
                        0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().rect_filled(
                        icon.translate(egui::vec2(-2.0, 2.0)).shrink(1.0),
                        0,
                        fill,
                    );
                    ui.painter().rect_stroke(
                        icon.translate(egui::vec2(-2.0, 2.0)).shrink(1.0),
                        0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                1 => {
                    ui.painter()
                        .rect_stroke(icon, 0, stroke, egui::StrokeKind::Inside);
                }
                _ => {
                    ui.painter()
                        .line_segment([icon.left_bottom(), icon.right_bottom()], stroke);
                }
            }
            response = response.on_hover_text(if !enabled && kind == 0 {
                if russian {
                    "Дождитесь сохранения файла"
                } else {
                    "Wait for the file to finish saving"
                }
            } else {
                label
            });
            if enabled && response.clicked() {
                ui.ctx().send_viewport_cmd(match kind {
                    0 => ViewportCommand::Close,
                    1 => ViewportCommand::Maximized(!maximized),
                    _ => ViewportCommand::Minimized(true),
                });
            }
        }
        let drag_rect = Rect::from_min_max(title.min, egui::pos2(right, title.bottom()));
        let response = ui.interact(
            drag_rect,
            Id::new("window-title-drag"),
            Sense::click_and_drag(),
        );
        ui.painter().with_clip_rect(drag_rect.shrink(8.0)).text(
            egui::pos2(drag_rect.left() + 10.0, drag_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Almavorn",
            egui::FontId::proportional(15.0),
            foreground,
        );
        if response.double_clicked_by(PointerButton::Primary) {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if response.drag_started_by(PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if !maximized {
            resize_edges(ui, outer, edge);
        }
        (content, keyboard_focus)
    }
}

fn resize_edges(ui: &mut egui::Ui, outer: Rect, edge: f32) {
    let Some(point) = ui.input(|input| input.pointer.hover_pos()) else {
        return;
    };
    if !outer.contains(point) || outer.shrink(edge).contains(point) {
        return;
    }
    let corner = 16.0;
    let left = point.x < outer.left() + corner;
    let right = point.x > outer.right() - corner;
    let top = point.y < outer.top() + corner;
    let bottom = point.y > outer.bottom() - corner;
    let (direction, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        _ if point.x < outer.left() + edge => (ResizeDirection::West, CursorIcon::ResizeHorizontal),
        _ if point.x > outer.right() - edge => {
            (ResizeDirection::East, CursorIcon::ResizeHorizontal)
        }
        _ if point.y < outer.top() + edge => (ResizeDirection::North, CursorIcon::ResizeVertical),
        _ => (ResizeDirection::South, CursorIcon::ResizeVertical),
    };
    ui.ctx().set_cursor_icon(cursor);
    if ui.input(|input| input.pointer.button_pressed(PointerButton::Primary)) {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}

fn desktop_colors() -> Option<Colors> {
    let mut roots = Vec::new();
    if let Some(path) = dirs::config_dir() {
        roots.push(path);
    }
    // Flatpak has an application-local XDG_CONFIG_HOME; its host desktop palette
    // remains readable under the real home with our existing filesystem access.
    if let Some(home) = dirs::home_dir() {
        let path = home.join(".config");
        if !roots.contains(&path) {
            roots.push(path);
        }
    }
    // Noctalia v5 exports the active palette here; its legacy colors.json can be stale.
    for relative in [
        "gtk-4.0/noctalia.css",
        "gtk-3.0/noctalia.css",
        "gtk-4.0/gtk.css",
        "gtk-3.0/gtk.css",
    ] {
        for root in &roots {
            let mut definitions = HashMap::new();
            read_colors(&root.join(relative), &mut definitions, &mut Vec::new());
            let resolve = |names: &[&str]| {
                names
                    .iter()
                    .find_map(|name| color_value(definitions.get(*name)?, &definitions, 0))
            };
            let Some(background) = resolve(&[
                "headerbar_bg_color",
                "theme_header_background_breeze",
                "window_bg_color",
                "theme_bg_color",
            ]) else {
                continue;
            };
            let Some(foreground) = resolve(&[
                "headerbar_fg_color",
                "theme_header_foreground_breeze",
                "window_fg_color",
                "theme_fg_color",
            ]) else {
                continue;
            };
            let Some(accent) = resolve(&[
                "accent_bg_color",
                "theme_selected_bg_color",
                "theme_selected_bg_color_breeze",
            ]) else {
                continue;
            };
            return Some(Colors {
                background,
                foreground: legible(foreground, background),
                accent,
                backdrop: resolve(&[
                    "headerbar_backdrop_color",
                    "theme_header_background_backdrop_breeze",
                ])
                .unwrap_or(background),
                on_accent: legible(
                    resolve(&["accent_fg_color", "theme_selected_fg_color"]).unwrap_or(foreground),
                    accent,
                ),
            });
        }
    }
    None
}

fn read_colors(path: &Path, definitions: &mut HashMap<String, String>, seen: &mut Vec<PathBuf>) {
    if seen.len() >= 16 || seen.iter().any(|item| item == path) {
        return;
    }
    seen.push(path.to_owned());
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return;
    }
    let Ok(css) = std::fs::read_to_string(path) else {
        return;
    };
    // Ignore CSS comments before looking for declarations or imports.
    let mut clean = String::new();
    let mut rest = css.as_str();
    while let Some((before, after)) = rest.split_once("/*") {
        clean.push_str(before);
        let Some((_, after)) = after.split_once("*/") else {
            rest = "";
            break;
        };
        rest = after;
    }
    clean.push_str(rest);
    for statement in clean.split(';') {
        let statement = statement.trim();
        if let Some(value) = statement.strip_prefix("@import") {
            let value = value
                .trim()
                .strip_prefix("url(")
                .unwrap_or(value.trim())
                .trim_end_matches(')')
                .trim();
            let value = value.trim_matches(['\'', '"']);
            if !value.is_empty() && !value.contains("://") && !value.starts_with("resource:") {
                let imported = Path::new(value);
                let imported = if imported.is_absolute() {
                    imported.to_owned()
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(imported)
                };
                read_colors(&imported, definitions, seen);
            }
        } else if let Some(value) = statement.strip_prefix("@define-color") {
            if let Some((name, value)) = value.trim().split_once(char::is_whitespace) {
                definitions.insert(name.to_owned(), value.trim().to_owned());
            }
        }
    }
}

fn color_value(
    value: &str,
    definitions: &HashMap<String, String>,
    depth: usize,
) -> Option<Color32> {
    if depth >= 16 {
        return None;
    }
    let value = value.trim();
    if let Some(name) = value.strip_prefix('@') {
        return color_value(definitions.get(name)?, definitions, depth + 1);
    }
    let hex = value.strip_prefix('#')?;
    let bytes = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        3 => Some(Color32::from_rgb(
            ((bytes >> 8) & 15) as u8 * 17,
            ((bytes >> 4) & 15) as u8 * 17,
            (bytes & 15) as u8 * 17,
        )),
        6 => Some(Color32::from_rgb(
            (bytes >> 16) as u8,
            (bytes >> 8) as u8,
            bytes as u8,
        )),
        _ => None,
    }
}

fn legible(preferred: Color32, background: Color32) -> Color32 {
    let luminance = |color: Color32| {
        let channel = |value: u8| {
            let c = f32::from(value) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    };
    let contrast = |a: Color32, b: Color32| {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    };
    if contrast(preferred, background) >= 4.5 {
        preferred
    } else if contrast(Color32::BLACK, background) > contrast(Color32::WHITE, background) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}
