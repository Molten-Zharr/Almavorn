use almavorn::{
    app::{App, Dialog, Target},
    color_picker::ColorPicker,
};
use eframe::egui;
use ratatui::layout::{Rect, Size};

pub fn map_rect(image: egui::Rect, size: Size, area: Rect) -> egui::Rect {
    let scale = egui::vec2(
        image.width() / f32::from(size.width),
        image.height() / f32::from(size.height),
    );
    egui::Rect::from_min_size(
        image.min + egui::vec2(f32::from(area.x) * scale.x, f32::from(area.y) * scale.y),
        egui::vec2(
            f32::from(area.width) * scale.x,
            f32::from(area.height) * scale.y,
        ),
    )
}

pub fn wheel_geometry(image: egui::Rect, size: Size, area: Rect) -> (egui::Pos2, f32) {
    let rect = map_rect(image, size, area);
    (
        rect.center(),
        (rect.width().min(rect.height()) * 0.5 - 1.0).max(1.0),
    )
}

#[derive(Default)]
pub struct ColorPickerPainter {
    wheel: Option<(usize, egui::TextureHandle)>,
}

impl ColorPickerPainter {
    pub fn paint(&mut self, ui: &egui::Ui, image: egui::Rect, app: &App, size: Size) {
        let Some(Dialog::Text(dialog)) = &app.view.dialog else {
            return;
        };
        let Some(color) = &dialog.color_picker else {
            return;
        };
        let painter = ui.painter().with_clip_rect(image.intersect(ui.clip_rect()));
        for hit in &app.view.hits {
            match hit.target {
                Target::ColorWheel(area) => {
                    let (center, radius) = wheel_geometry(image, size, area);
                    let pixels = (radius * 2.0 * painter.pixels_per_point())
                        .round()
                        .clamp(8.0, 512.0) as usize;
                    if self
                        .wheel
                        .as_ref()
                        .is_none_or(|(width, _)| *width != pixels)
                    {
                        let mut colors = Vec::with_capacity(pixels * pixels);
                        for y in 0..pixels {
                            for x in 0..pixels {
                                let nx = 2.0 * (x as f64 + 0.5) / pixels as f64 - 1.0;
                                let ny = 2.0 * (y as f64 + 0.5) / pixels as f64 - 1.0;
                                colors.push(ColorPicker::wheel_rgb(nx, ny).map_or(
                                    egui::Color32::TRANSPARENT,
                                    |[r, g, b]| {
                                        let alpha = ((1.0 - nx.hypot(ny)) * pixels as f64 * 0.5)
                                            .clamp(0.0, 1.0);
                                        egui::Color32::from_rgba_unmultiplied(
                                            r,
                                            g,
                                            b,
                                            (alpha * 255.0).round() as u8,
                                        )
                                    },
                                ));
                            }
                        }
                        let texture = ui.ctx().load_texture(
                            "palette-color-wheel",
                            egui::ColorImage::new([pixels, pixels], colors),
                            egui::TextureOptions::LINEAR,
                        );
                        self.wheel = Some((pixels, texture));
                    }
                    painter.image(
                        self.wheel.as_ref().unwrap().1.id(),
                        egui::Rect::from_center_size(
                            center,
                            egui::vec2(radius * 2.0, radius * 2.0),
                        ),
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                    let angle = color.hsv.hue.to_radians();
                    let marker = center
                        + egui::vec2(angle.cos() as f32, angle.sin() as f32)
                            * color.hsv.saturation as f32
                            * radius;
                    painter.circle_stroke(
                        marker,
                        5.0,
                        egui::Stroke::new(3.0_f32, egui::Color32::BLACK),
                    );
                    painter.circle_stroke(
                        marker,
                        5.0,
                        egui::Stroke::new(1.0_f32, egui::Color32::WHITE),
                    );
                }
                Target::ColorSlider(index, area) => {
                    let rect = map_rect(image, size, area).shrink2(egui::vec2(0.0, 1.0));
                    let mut mesh = egui::Mesh::default();
                    for segment in 0..64 {
                        let left = segment as f64 / 64.0;
                        let right = (segment + 1) as f64 / 64.0;
                        let a = color.gradient(index, left);
                        let b = color.gradient(index, right);
                        let left = rect.left() + rect.width() * left as f32;
                        let right = rect.left() + rect.width() * right as f32;
                        let first = mesh.vertices.len() as u32;
                        for (pos, rgb) in [
                            (egui::pos2(left, rect.top()), a),
                            (egui::pos2(right, rect.top()), b),
                            (egui::pos2(left, rect.bottom()), a),
                            (egui::pos2(right, rect.bottom()), b),
                        ] {
                            mesh.colored_vertex(
                                pos,
                                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                            );
                        }
                        mesh.add_triangle(first, first + 1, first + 2);
                        mesh.add_triangle(first + 1, first + 3, first + 2);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    let cell_width = image.width() / f32::from(size.width);
                    let ratio =
                        (color.channel(index) / ColorPicker::maximum(index)).clamp(0.0, 1.0) as f32;
                    let x = rect.left() + cell_width * 0.5 + ratio * (rect.width() - cell_width);
                    let line = [
                        egui::pos2(x, rect.top() - 1.0),
                        egui::pos2(x, rect.bottom() + 1.0),
                    ];
                    painter.line_segment(line, egui::Stroke::new(3.0_f32, egui::Color32::BLACK));
                    painter.line_segment(line, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
                }
                _ => {}
            }
        }
    }
}
