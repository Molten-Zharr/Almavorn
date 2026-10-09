use almavorn::{
    app::{App, Target},
    preferences::Corners,
};
use eframe::egui;
use egui_ratatui::RataguiBackend;
use ratatui::{buffer::Buffer, layout::Rect};
use soft_ratatui::{RasterBackend, RgbPixmap, SoftBackend};
use std::sync::Arc;

type WaveformKey = (usize, usize, egui::Rect, egui::Color32, egui::Color32);

#[derive(Default)]
pub struct PlaybackPainter {
    waveform_key: Option<WaveformKey>,
    waveform_meshes: Option<(Arc<egui::Mesh>, Arc<egui::Mesh>)>,
}

impl PlaybackPainter {
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        image: egui::Rect,
        app: &App,
        size: ratatui::layout::Size,
    ) {
        if (app.view.dialog.is_some() && app.view.overlay_area.is_empty())
            || (app.view.progress_area.is_empty() && app.view.waveform_area.is_empty())
        {
            return;
        }
        let scale = egui::vec2(
            image.width() / f32::from(size.width),
            image.height() / f32::from(size.height),
        );
        let map = |area: Rect| {
            egui::Rect::from_min_size(
                image.min + egui::vec2(f32::from(area.x) * scale.x, f32::from(area.y) * scale.y),
                egui::vec2(
                    f32::from(area.width) * scale.x,
                    f32::from(area.height) * scale.y,
                ),
            )
        };
        let waveform = !app.view.waveform_area.is_empty();
        let wave = map(if waveform {
            app.view.waveform_area
        } else {
            app.view.progress_area
        });
        let palette = app.settings.current_palette();
        let color = |key| {
            let [r, g, b] = palette.color(key);
            egui::Color32::from_rgb(r, g, b)
        };
        let background = color("background");
        let active = color("active");
        let future = color("inactive_panel_border");
        let duration = app
            .playback
            .current
            .as_ref()
            .map_or(0, |track| track.duration_ms);
        let ratio = if duration > 0 {
            (app.position_ms() as f64 / duration as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let x = wave.left() + ratio * wave.width();
        let painter = ui.painter().with_clip_rect(image.intersect(ui.clip_rect()));
        if let Some(values) = app
            .playback
            .waveform
            .as_deref()
            .filter(|values| waveform && !values.is_empty())
        {
            painter.rect_filled(wave, 0, background);
            let key = (values.as_ptr() as usize, values.len(), wave, active, future);
            if self.waveform_key != Some(key) {
                let columns = (wave.width() * ui.painter().pixels_per_point())
                    .ceil()
                    .clamp(2.0, 8192.0) as usize;
                let peak = values
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
                    .max(f32::EPSILON);
                let mut mesh = egui::Mesh::default();
                let amplitude = (wave.height() / 2.0 - 3.0).max(0.0);
                for column in 0..=columns {
                    let start = (column * values.len() / (columns + 1)).min(values.len() - 1);
                    let end = (((column + 1) * values.len() / (columns + 1)).max(start + 1))
                        .min(values.len());
                    let level = values[start..end].iter().copied().fold(0.0f32, f32::max) / peak;
                    let x = wave.left() + column as f32 * wave.width() / columns as f32;
                    let height = level.clamp(0.0, 1.0) * amplitude;
                    for y in [wave.center().y - height, wave.center().y + height] {
                        mesh.colored_vertex(egui::pos2(x, y), future);
                    }
                    if column > 0 {
                        let a = (column as u32 - 1) * 2;
                        mesh.add_triangle(a, a + 1, a + 2);
                        mesh.add_triangle(a + 1, a + 3, a + 2);
                    }
                }
                let mut played = mesh.clone();
                for vertex in &mut played.vertices {
                    vertex.color = active;
                }
                self.waveform_meshes = Some((Arc::new(mesh), Arc::new(played)));
                self.waveform_key = Some(key);
            }
            if let Some((future_mesh, played_mesh)) = &self.waveform_meshes {
                painter.add(egui::Shape::mesh(future_mesh.clone()));
                if ratio > 0.0 {
                    painter
                        .with_clip_rect(
                            egui::Rect::from_min_max(wave.min, egui::pos2(x, wave.bottom()))
                                .intersect(painter.clip_rect()),
                        )
                        .add(egui::Shape::mesh(played_mesh.clone()));
                }
            }
            painter.line_segment(
                [
                    egui::pos2(wave.left(), wave.center().y),
                    egui::pos2(wave.right(), wave.center().y),
                ],
                egui::Stroke::new(0.5_f32, future),
            );
        } else {
            self.waveform_key = None;
            self.waveform_meshes = None;
        }
        if !waveform {
            painter.rect_filled(wave, 0, future);
            if ratio > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_max(wave.min, egui::pos2(x, wave.bottom())),
                    0,
                    active,
                );
            }
        }
        if app.playback_active() && (!waveform || self.waveform_meshes.is_some()) {
            let x = x.clamp(wave.left() + 0.5, wave.right() - 0.5);
            let painter = painter.with_clip_rect(wave.intersect(painter.clip_rect()));
            if waveform {
                painter.line_segment(
                    [
                        egui::pos2(x, wave.top() + 2.0),
                        egui::pos2(x, wave.bottom() - 2.0),
                    ],
                    egui::Stroke::new(1.0 / ui.painter().pixels_per_point(), active),
                );
            }
            painter.circle_filled(egui::pos2(x, wave.center().y), 2.5, active);
        }
    }
}

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

    pub fn update<R: RasterBackend>(
        &mut self,
        ctx: &egui::Context,
        backend: &mut RataguiBackend<R>,
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

pub fn resize<R: RasterBackend>(backend: &mut SoftBackend<R>, columns: u16, rows: u16) {
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
    for hit in &app.view.hits {
        if app.view.dialog.is_none()
            && matches!(hit.target, Target::Action(_) | Target::Mode(_))
            && app
                .view
                .workspace
                .areas
                .iter()
                .any(|(_, area)| hit.area.y > area.y && hit.area.intersection(*area) == hit.area)
        {
            continue;
        }
        if hit.area.height != 1
            || matches!(hit.target, Target::Setting(_)) && hit.area.width == 1
            || !matches!(
                hit.target,
                Target::Action(_)
                    | Target::Mode(_)
                    | Target::Submit
                    | Target::SearchPlay
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
                    | Target::SettingsTimeline(_, _)
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
        } else if app.hovered(hit.area)
            || matches!(hit.target, Target::Action(almavorn::input::Action::Panels))
        {
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
    for cap in &app.view.keycaps {
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
        // The backing raster is blank here; key faces are drawn only by the GUI.
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

#[cfg(test)]
mod tests {
    use super::*;
    use almavorn::{
        app::SettingsPage,
        input::{Action, Input, Key, KeyPress},
        ui,
    };
    use ratatui::Terminal;

    #[test]
    fn mouse_highlights_and_dialog_changes_leave_no_pixels_in_cached_frames() {
        for (columns, rows) in [(120, 50), (70, 30)] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new(directory.path()).unwrap();
            app.view.graphical_keycaps = true;
            let mut terminal = Terminal::new(RataguiBackend::new(
                "selection-regression",
                crate::gui_fonts::backend(
                    app.settings.appearance.font,
                    app.settings.appearance.font_size,
                    columns,
                    rows,
                ),
            ))
            .unwrap();
            let ctx = egui::Context::default();
            let mut cache = ImageCache::default();
            let mut uploaded = egui::ColorImage::default();
            let mut frame = |app: &mut App| {
                terminal.draw(|frame| ui::render(app, frame)).unwrap();
                let backend = terminal.backend_mut();
                cache.update(&ctx, backend);
                let texture = backend.text_handle.as_ref().unwrap().id();
                for (id, delta) in ctx.tex_manager().write().take_delta().set {
                    if id != texture {
                        continue;
                    }
                    let egui::ImageData::Color(image) = delta.image;
                    if let Some([x, y]) = delta.pos {
                        for row in 0..image.height() {
                            let start = (y + row) * uploaded.width() + x;
                            uploaded.pixels[start..start + image.width()].copy_from_slice(
                                &image.pixels[row * image.width()..(row + 1) * image.width()],
                            );
                        }
                    } else {
                        uploaded = (*image).clone();
                    }
                }
                let mut fresh = crate::gui_fonts::backend(
                    app.settings.appearance.font,
                    app.settings.appearance.font_size,
                    columns,
                    rows,
                );
                fresh.buffer = backend.soft_backend.buffer.clone();
                fresh.redraw();
                let expected = egui::ColorImage::from_rgb(
                    [fresh.get_pixmap_width(), fresh.get_pixmap_height()],
                    fresh.get_pixmap_data(),
                );
                let mismatch = uploaded
                    .pixels
                    .iter()
                    .zip(&expected.pixels)
                    .position(|(actual, expected)| actual != expected);
                assert_eq!(uploaded.size, expected.size);
                assert!(
                    mismatch.is_none(),
                    "{columns} columns, dialog={}: stale pixel {mismatch:?}",
                    app.view.dialog.is_some()
                );
            };
            frame(&mut app);
            for action in [
                Action::PlaybackTuner,
                Action::Equalizer,
                Action::Settings,
                Action::Search,
                Action::Help,
            ] {
                app.action(action).unwrap();
                frame(&mut app);
                let areas: Vec<_> = app
                    .view
                    .hits
                    .iter()
                    .filter(|hit| hit.enabled)
                    .map(|hit| hit.area)
                    .collect();
                for area in areas {
                    app.handle(Input::Move {
                        x: area.x,
                        y: area.y,
                    });
                    frame(&mut app);
                    if let Some(target @ Target::TunerSlider(_, _)) =
                        app.target_at((area.x, area.y).into()).cloned()
                    {
                        let Target::TunerSlider(index, bar) = target else {
                            unreachable!()
                        };
                        app.drag_tuner(index, bar, 0.47, false);
                        frame(&mut app);
                        app.drag_tuner(index, bar, 0.63, true);
                        frame(&mut app);
                    }
                    app.handle(Input::Move {
                        x: u16::MAX,
                        y: u16::MAX,
                    });
                    frame(&mut app);
                }
                app.handle(Input::Key(KeyPress::plain(Key::Escape)));
                frame(&mut app);
            }
            for page in SettingsPage::ALL {
                app.open_settings_page(page);
                frame(&mut app);
            }
            app.open_settings_page(SettingsPage::Palettes);
            frame(&mut app);
            let sample = app
                .view
                .hits
                .iter()
                .find(|hit| matches!(hit.target, Target::Setting(4)))
                .unwrap()
                .area;
            app.handle(Input::Click {
                x: sample.x,
                y: sample.y,
                double: false,
            });
            app.handle(Input::Release {
                x: sample.x,
                y: sample.y,
            });
            frame(&mut app);
            for index in 0..6 {
                while !matches!(&app.view.dialog, Some(almavorn::app::Dialog::Text(dialog)) if dialog.color_picker.as_ref().unwrap().selected == Some(index))
                {
                    app.handle(Input::Key(KeyPress::plain(Key::Tab)));
                }
                frame(&mut app);
                let bar = app
                    .view
                    .hits
                    .iter()
                    .find_map(|hit| match hit.target {
                        Target::ColorSlider(channel, area) if channel == index => Some(area),
                        _ => None,
                    })
                    .unwrap();
                for ratio in [0.0, 0.39, 0.4, 0.41, 1.0] {
                    app.drag_color_slider(index, bar, ratio, true);
                    frame(&mut app);
                }
            }
            app.handle(Input::Key(KeyPress::plain(Key::Escape)));
            frame(&mut app);
        }
    }
}
