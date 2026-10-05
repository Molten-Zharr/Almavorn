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
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                backend.text_handle =
                    Some(ctx.load_texture("almavorn", image, egui::TextureOptions::LINEAR));
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
                        egui::TextureOptions::LINEAR,
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
        );
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
}
