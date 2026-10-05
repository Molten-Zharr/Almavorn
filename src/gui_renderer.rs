use almavorn::{
    app::{App, Target},
    preferences::Corners,
};
use eframe::egui;
use egui_ratatui::RataguiBackend;
use ratatui::{buffer::Buffer, layout::Rect};
use soft_ratatui::{EmbeddedTTF, RgbPixmap, SoftBackend};

#[derive(Default)]
pub struct ImageCache {
    buffer: Option<Buffer>,
    cursor: Option<(u16, u16)>,
    blink: (bool, bool),
}

impl ImageCache {
    pub fn invalidate(&mut self) {
        self.buffer = None;
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        backend: &mut RataguiBackend<EmbeddedTTF>,
    ) -> egui::Vec2 {
        let soft = &backend.soft_backend;
        let size = [soft.get_pixmap_width(), soft.get_pixmap_height()];
        let blink = (
            soft.blink_config.fast.is_hidden(),
            soft.blink_config.slow.is_hidden(),
        );
        let full = self
            .buffer
            .as_ref()
            .is_none_or(|buffer| buffer.area != soft.buffer.area)
            || backend
                .text_handle
                .as_ref()
                .is_none_or(|texture| texture.size() != size);
        if full {
            let image = backend.to_egui_image();
            if let Some(texture) = &mut backend.text_handle {
                texture.set(image, egui::TextureOptions::NEAREST);
            } else {
                backend.text_handle =
                    Some(ctx.load_texture("almavorn", image, egui::TextureOptions::NEAREST));
            }
            self.buffer = Some(backend.soft_backend.buffer.clone());
        } else {
            let mut damage = None;
            let previous = self.buffer.as_mut().expect("uploaded cell buffer exists");
            for (index, (old, new)) in previous
                .content
                .iter_mut()
                .zip(&soft.buffer.content)
                .enumerate()
            {
                if old != new {
                    let (x, y) = soft.buffer.pos_of(index);
                    include_cell(&mut damage, x, y, soft.buffer.area);
                    *old = new.clone();
                }
            }
            if self.blink != blink {
                for &(x, y) in &soft.always_redraw_list {
                    include_cell(&mut damage, x, y, soft.buffer.area);
                }
            }
            // Cursor pixels can change without a change to a Ratatui cell.
            if self.cursor != soft.rendered_cursor {
                for (x, y) in self.cursor.into_iter().chain(soft.rendered_cursor) {
                    include_cell(&mut damage, x, y, soft.buffer.area);
                }
            }
            if let Some(rect) = damage {
                let left = usize::from(rect.x) * soft.char_width;
                let top = usize::from(rect.y) * soft.char_height;
                let width = usize::from(rect.width) * soft.char_width;
                let height = usize::from(rect.height) * soft.char_height;
                let mut pixels = Vec::with_capacity(width * height);
                let data = soft.get_pixmap_data();
                for y in top..top + height {
                    let start = (y * size[0] + left) * 3;
                    pixels.extend(
                        data[start..start + width * 3]
                            .as_chunks::<3>()
                            .0
                            .iter()
                            .map(|rgb| egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])),
                    );
                }
                backend
                    .text_handle
                    .as_mut()
                    .expect("uploaded texture exists")
                    .set_partial(
                        [left, top],
                        egui::ColorImage::new([width, height], pixels),
                        egui::TextureOptions::NEAREST,
                    );
            }
        }
        self.cursor = backend.soft_backend.rendered_cursor;
        self.blink = blink;
        egui::vec2(size[0] as f32, size[1] as f32)
    }
}

fn include_cell(damage: &mut Option<Rect>, x: u16, y: u16, area: Rect) {
    if x < area.right() && y < area.bottom() {
        // A wide glyph can also occupy the following character cell.
        let cell = Rect::new(x, y, 2.min(area.right() - x), 1);
        *damage = Some(damage.map_or(cell, |rect| rect.union(cell)));
    }
}

pub fn resize(backend: &mut SoftBackend<EmbeddedTTF>, columns: u16, rows: u16) {
    backend.buffer.resize(Rect::new(0, 0, columns, rows));
    backend.rgb_pixmap = RgbPixmap::new(
        backend.char_width * usize::from(columns),
        backend.char_height * usize::from(rows),
    );
    backend.always_redraw_list.clear();
    backend.rendered_cursor = None;
    // Terminal::draw will render the new layout. SoftBackend::resize would
    // rasterize the old layout first, only for Terminal to clear it immediately.
}

pub fn paint_keycaps(ui: &egui::Ui, image: egui::Rect, app: &App, size: ratatui::layout::Size) {
    let palette = app.settings.current_palette();
    for hit in &app.hits {
        if app.dialog.is_none()
            && matches!(hit.target, Target::Action(_) | Target::Mode(_))
            && app
                .workspace
                .areas
                .iter()
                .any(|(_, area)| hit.area.y > area.y && hit.area.intersection(*area) == hit.area)
        {
            continue;
        }
        if hit.area.height != 1
            || !matches!(
                hit.target,
                Target::Action(_)
                    | Target::Mode(_)
                    | Target::Submit
                    | Target::CloseDialog
                    | Target::Text(_)
                    | Target::Backspace
                    | Target::KeyboardLanguage
                    | Target::KeyboardCase
                    | Target::BrowserParent
                    | Target::BrowserMarkAll
                    | Target::BrowserOpen
                    | Target::BrowserAdd
                    | Target::DialogScroll(_)
                    | Target::Setting(_)
                    | Target::SettingAdjust(_, _)
                    | Target::SettingHelp(_)
                    | Target::BindingModifier(_)
                    | Target::BindingKey(_)
            )
        {
            continue;
        }
        let scale = egui::vec2(
            image.width() / f32::from(size.width),
            image.height() / f32::from(size.height),
        );
        let min = image.min
            + egui::vec2(
                f32::from(hit.area.x) * scale.x,
                f32::from(hit.area.y) * scale.y,
            );
        let rect = egui::Rect::from_min_size(
            min,
            egui::vec2(f32::from(hit.area.width) * scale.x, scale.y),
        )
        .shrink(1.0 / ui.painter().pixels_per_point());
        let [r, g, b] = palette.color(if !hit.enabled {
            "folder_path"
        } else if app.hovered(hit.area) {
            "active"
        } else {
            "inactive_panel_border"
        });
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(if app.settings.appearance.corners == Corners::Rounded {
                4
            } else {
                0
            }),
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(r, g, b)),
            egui::StrokeKind::Inside,
        );
    }
    let scale = egui::vec2(
        image.width() / f32::from(size.width),
        image.height() / f32::from(size.height),
    );
    let face_rgb = palette.color("selected_file_background");
    let shade = |target: u8, amount: u16| {
        let color = face_rgb.map(|value| {
            ((u16::from(value) * (100 - amount) + u16::from(target) * amount) / 100) as u8
        });
        egui::Color32::from_rgb(color[0], color[1], color[2])
    };
    for cap in &app.keycaps {
        let cell_rect = egui::Rect::from_min_size(
            image.min
                + egui::vec2(
                    f32::from(cap.area.x) * scale.x,
                    f32::from(cap.area.y) * scale.y,
                ),
            egui::vec2(
                f32::from(cap.area.width) * scale.x,
                f32::from(cap.area.height) * scale.y,
            ),
        );
        // The GUI key covers the terminal key, including its edge glyphs.
        let [r, g, b] = palette.color("background");
        ui.painter()
            .rect_filled(cell_rect, 0, egui::Color32::from_rgb(r, g, b));
        let rect = cell_rect.shrink(1.0);
        let bevel = (rect.height() * 0.13).max(2.0);
        let face = egui::Rect::from_min_max(
            rect.min + egui::vec2(bevel, bevel),
            rect.max - egui::vec2(bevel, bevel * 1.65),
        );
        ui.painter().rect_filled(rect, 0, shade(0, 45));
        for (points, color) in [
            (
                vec![
                    rect.left_top(),
                    rect.right_top(),
                    face.right_top(),
                    face.left_top(),
                ],
                shade(255, 25),
            ),
            (
                vec![
                    rect.left_top(),
                    face.left_top(),
                    face.left_bottom(),
                    rect.left_bottom(),
                ],
                shade(255, 12),
            ),
            (
                vec![
                    rect.right_top(),
                    rect.right_bottom(),
                    face.right_bottom(),
                    face.right_top(),
                ],
                shade(0, 30),
            ),
            (
                vec![
                    rect.left_bottom(),
                    face.left_bottom(),
                    face.right_bottom(),
                    rect.right_bottom(),
                ],
                shade(0, 45),
            ),
        ] {
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                color,
                egui::Stroke::NONE,
            ));
        }
        ui.painter().rect_filled(face, 0, shade(0, 0));
        let [r, g, b] = palette.color("button_text");
        ui.painter().text(
            face.center(),
            egui::Align2::CENTER_CENTER,
            &cap.label,
            egui::FontId::monospace((f32::from(app.settings.appearance.font_size) * 0.75).max(8.0)),
            egui::Color32::from_rgb(r, g, b),
        );
    }
}
