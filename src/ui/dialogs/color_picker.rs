use super::super::{
    buttons::{quiet_button, quiet_width},
    theme::Palette,
    widgets::{modal, visible_offset},
};
use crate::{
    app::{App, Hit, SettingsEdit, Target, TextDialog, TextPurpose},
    color_picker::ColorPicker,
    preferences::{color_role_name, parse_color},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Color,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    dialog: &TextDialog,
    area: Rect,
    palette: Palette,
) {
    let color = dialog
        .color_picker
        .as_ref()
        .expect("color dialog has picker state");
    let role =
        if let TextPurpose::Settings(SettingsEdit::PaletteColor { token, .. }) = &dialog.purpose {
            color_role_name(token, app.settings.language)
        } else {
            ""
        };
    let title = format!("{} · {role}", app.text("Color wheel", "Цветовой круг"));
    let inner = modal(frame, app, area, title, 33, palette);
    if inner.is_empty() {
        return;
    }
    let header_height = if inner.height >= 18 { 2 } else { 0 };
    let preview_height = if inner.height >= 18 { 2 } else { 1 };
    let hex_height = if inner.height >= 12 { 3 } else { 1 };
    let keypad_height = if inner.height >= 24 { 2 } else { 0 };
    let footer_height = if inner.height >= 12 { 2 } else { 1 };
    let body_height = inner.height.saturating_sub(
        header_height + preview_height + hex_height + keypad_height + footer_height,
    );
    if header_height > 0 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Wheel: hue and saturation · Tab: controls · Arrows: adjust",
                "Круг: оттенок и насыщенность · Tab: параметры · Стрелки: шаг",
            ))
            .style(palette.text().fg(palette.muted)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }
    let body = Rect::new(inner.x, inner.y + header_height, inner.width, body_height);
    if body.height >= 2 && body.width >= 12 {
        let wheel_width = (body.width / 2).min(36).min(body.height * 2) / 2 * 2;
        let wheel_height = wheel_width / 2;
        let wheel = Rect::new(
            body.x,
            body.y + (body.height - wheel_height) / 2,
            wheel_width,
            wheel_height,
        );
        frame.render_widget(Block::default().style(palette.text()), wheel);
        if !app.view.graphical_keycaps {
            draw_wheel(frame, color, wheel, palette);
        }
        app.view.hits.push(Hit {
            area: wheel,
            target: Target::ColorWheel(wheel),
            enabled: true,
        });
        let controls = Rect::new(
            wheel.right() + 2,
            body.y,
            body.width.saturating_sub(wheel.width + 2),
            body.height,
        );
        let step = if controls.height < 12 {
            1
        } else if controls.width >= 28 && controls.height >= 18 {
            3
        } else {
            2
        };
        let capacity = usize::from(controls.height / step).max(1);
        let offset = visible_offset(
            0,
            color.selected.unwrap_or(0),
            capacity,
            ColorPicker::CHANNELS,
        );
        for index in offset..(offset + capacity).min(ColorPicker::CHANNELS) {
            let y = controls.y + (index - offset) as u16 * step;
            if y + step > controls.bottom() {
                break;
            }
            let unit = match index {
                0 => "°",
                1 | 2 => "%",
                _ => "",
            };
            let label = format!(
                "{}: {:.0}{unit}",
                ColorPicker::label(
                    index,
                    app.settings.language,
                    step == 1 || controls.width < 28
                ),
                color.channel(index)
            );
            let label_width = if step == 1 {
                9.min(controls.width / 2)
            } else {
                controls.width
            };
            let label_area = Rect::new(controls.x, y, label_width, 1);
            let style = if color.selected == Some(index) {
                palette.text().bg(palette.selection).fg(palette.accent)
            } else {
                palette.text()
            };
            frame.render_widget(Paragraph::new(label).style(style), label_area);
            app.view.hits.push(Hit {
                area: label_area,
                target: Target::ColorSelect(index),
                enabled: true,
            });
            let bar = if step == 1 {
                Rect::new(controls.x + label_width, y, controls.width - label_width, 1)
            } else {
                Rect::new(controls.x, y + 1, controls.width, 1)
            };
            for x in 0..bar.width {
                let ratio = f64::from(x) / f64::from(bar.width.saturating_sub(1).max(1));
                let [r, g, b] = color.gradient(index, ratio);
                let position = (color.channel(index) / ColorPicker::maximum(index)
                    * f64::from(bar.width.saturating_sub(1)))
                .round() as u16;
                frame.render_widget(
                    Paragraph::new(if x == position { "│" } else { " " }).style(
                        style.bg(Color::Rgb(r, g, b)).fg(
                            if u32::from(r) + u32::from(g) + u32::from(b) > 384 {
                                Color::Black
                            } else {
                                Color::White
                            },
                        ),
                    ),
                    Rect::new(bar.x + x, bar.y, 1, 1),
                );
            }
            app.view.hits.push(Hit {
                area: bar,
                target: Target::ColorSlider(index, bar),
                enabled: true,
            });
        }
    }
    let preview_y = body.bottom();
    let half = inner.width / 2;
    let original_area = Rect::new(inner.x, preview_y, half, 1);
    let rgb = color.hsv.rgb();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "██",
                palette
                    .text()
                    .bg(Color::Rgb(
                        color.original[0],
                        color.original[1],
                        color.original[2],
                    ))
                    .fg(Color::Rgb(
                        color.original[0],
                        color.original[1],
                        color.original[2],
                    )),
            ),
            Span::raw(format!(
                " {}",
                app.text("Original / reset", "Исходный / сброс")
            )),
        ]))
        .style(palette.text()),
        original_area,
    );
    app.view.hits.push(Hit {
        area: original_area,
        target: Target::ColorReset,
        enabled: true,
    });
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "██",
                palette
                    .text()
                    .bg(Color::Rgb(rgb[0], rgb[1], rgb[2]))
                    .fg(Color::Rgb(rgb[0], rgb[1], rgb[2])),
            ),
            Span::raw(format!(" {}", app.text("New color", "Новый цвет"))),
        ]))
        .style(palette.text()),
        Rect::new(
            inner.x + half + 1,
            preview_y,
            inner.width.saturating_sub(half + 1),
            1,
        ),
    );
    let valid = parse_color(&dialog.text).is_ok();
    let hex = Rect::new(inner.x, preview_y + preview_height, inner.width, hex_height);
    let mut field = Paragraph::new(format!(
        "{}{}",
        dialog.text,
        if color.selected.is_none() { "│" } else { "" }
    ))
    .style(
        palette
            .text()
            .fg(if valid { palette.text } else { palette.error })
            .bg(if color.selected.is_none() && dialog.selected_all {
                palette.selection
            } else {
                palette.panel
            }),
    );
    if hex.height >= 3 {
        field = field.block(Block::default().borders(Borders::ALL).title(" HEX "));
    }
    frame.render_widget(field, hex);
    app.view.hits.push(Hit {
        area: hex,
        target: Target::ColorHex,
        enabled: true,
    });
    if keypad_height > 0 {
        let mut x = inner.x;
        let mut y = hex.bottom();
        let bottom = y + keypad_height;
        for (label, target) in "0123456789ABCDEF"
            .chars()
            .map(|c| (c.to_string(), Target::Text(c)))
            .chain(std::iter::once(("←".into(), Target::Backspace)))
        {
            let width = quiet_width(&label);
            if x + width > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= bottom {
                break;
            }
            quiet_button(
                frame,
                app,
                Rect::new(x, y, width, 1),
                &label,
                target,
                true,
                palette,
            );
            x += width + 1;
        }
    }
    let footer_y = inner.bottom() - 1;
    quiet_button(
        frame,
        app,
        Rect::new(inner.x, footer_y, half, 1),
        if inner.width < 28 {
            "OK"
        } else {
            app.text("Apply", "Применить")
        },
        Target::Submit,
        valid,
        palette,
    );
    quiet_button(
        frame,
        app,
        Rect::new(
            inner.x + half + 1,
            footer_y,
            inner.width.saturating_sub(half + 1),
            1,
        ),
        if inner.width < 28 {
            "←"
        } else {
            app.text("Cancel", "Отмена")
        },
        Target::CloseDialog,
        true,
        palette,
    );
}

fn draw_wheel(frame: &mut Frame, color: &ColorPicker, area: Rect, palette: Palette) {
    for y in 0..area.height {
        for x in 0..area.width {
            let nx = 2.0 * (f64::from(x) + 0.5) / f64::from(area.width) - 1.0;
            let top_y = 2.0 * (f64::from(y) + 0.25) / f64::from(area.height) - 1.0;
            let bottom_y = 2.0 * (f64::from(y) + 0.75) / f64::from(area.height) - 1.0;
            let map = |rgb: Option<[u8; 3]>| {
                rgb.map_or(palette.background, |[r, g, b]| Color::Rgb(r, g, b))
            };
            frame.render_widget(
                Paragraph::new("▀").style(
                    palette
                        .text()
                        .fg(map(ColorPicker::wheel_rgb(nx, top_y)))
                        .bg(map(ColorPicker::wheel_rgb(nx, bottom_y))),
                ),
                Rect::new(area.x + x, area.y + y, 1, 1),
            );
        }
    }
    let angle = color.hsv.hue.to_radians();
    let x = ((angle.cos() * color.hsv.saturation + 1.0) * 0.5 * f64::from(area.width - 1)).round()
        as u16;
    let y = ((angle.sin() * color.hsv.saturation + 1.0) * 0.5 * f64::from(area.height - 1)).round()
        as u16;
    frame.render_widget(
        Paragraph::new("○").style(palette.text().fg(Color::White)),
        Rect::new(area.x + x, area.y + y, 1, 1),
    );
}
