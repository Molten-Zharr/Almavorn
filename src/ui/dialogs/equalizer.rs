use super::super::{
    theme::Palette,
    widgets::{block, buttons, modal, visible_offset},
};
use crate::{
    app::{App, Hit, Target},
    equalizer::{LABELS, MAX_GAIN, MIN_GAIN, PRESETS, SLIDERS},
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::{Clear, Paragraph},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    selected: &mut usize,
    presets: bool,
    preset_selected: &mut usize,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Equalizer", "Эквалайзер").into(),
        25,
        palette,
    );
    if inner.width < 8 || inner.height < 6 {
        buttons(
            frame,
            app,
            inner,
            vec![(
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            )],
            palette,
        );
        return;
    }
    *selected = (*selected).min(SLIDERS - 1);
    let name = app.settings.equalizer.preset().map_or_else(
        || app.text("Custom", "Пользовательский"),
        |index| PRESETS[index].name(app.settings.language),
    );
    let entries = vec![
        (
            format!(
                "[{}] {}",
                if app.settings.equalizer.enabled {
                    'x'
                } else {
                    ' '
                },
                app.text("On", "Включён")
            ),
            Target::EqualizerToggle,
            true,
        ),
        (format!("{name} ↓"), Target::EqualizerPresets, true),
        (
            app.text("Reset", "Сброс").into(),
            Target::EqualizerReset,
            true,
        ),
    ];
    let header_height = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y, inner.width, 3),
        entries,
        palette,
    );
    let body = Rect::new(
        inner.x,
        inner.y + header_height + 1,
        inner.width,
        inner.height.saturating_sub(header_height + 4),
    );
    if body.width >= 70 && body.height >= 8 {
        vertical(frame, app, *selected, body, palette);
    } else {
        horizontal(frame, app, *selected, body, palette);
    }
    frame.render_widget(
        Paragraph::new(app.text(
            "←→: band · ↑↓/wheel: 1 dB · 1–8: preset",
            "←→: полоса · ↑↓/колесо: 1 дБ · 1–8: пресет",
        ))
        .style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        vec![(
            app.text("Close", "Закрыть").into(),
            Target::CloseDialog,
            true,
        )],
        palette,
    );
    if presets {
        dropdown(frame, app, preset_selected, inner, palette);
    }
}

fn label(app: &App, index: usize) -> &'static str {
    if index == 0 {
        app.text("Preamp", "Пред.")
    } else {
        LABELS[index - 1]
    }
}

fn vertical(frame: &mut Frame, app: &mut App, selected: usize, area: Rect, palette: Palette) {
    let width = area.width.saturating_sub(5);
    let bar_height = area.height.saturating_sub(3);
    for (name, y) in [
        ("+12", area.y + 2),
        ("0", area.y + 2 + (bar_height - 1) / 2),
        ("−12", area.y + 2 + bar_height - 1),
    ] {
        frame.render_widget(
            Paragraph::new(name).style(palette.text().fg(palette.muted)),
            Rect::new(area.x, y, 4, 1),
        );
    }
    for index in 0..SLIDERS {
        let start = area.x + 5 + (u32::from(width) * index as u32 / SLIDERS as u32) as u16;
        let end = area.x + 5 + (u32::from(width) * (index + 1) as u32 / SLIDERS as u32) as u16;
        let bar = Rect::new(start, area.y + 2, end - start, bar_height);
        let style = if index == selected {
            palette.text().bg(palette.selection).fg(palette.accent)
        } else {
            palette.text()
        };
        let gain = app.settings.equalizer.gain(index).unwrap_or(0);
        frame.render_widget(
            Paragraph::new(format!("{gain:+}"))
                .alignment(Alignment::Center)
                .style(style),
            Rect::new(start, area.y, bar.width, 1),
        );
        let knob = (f32::from(MAX_GAIN - gain) * f32::from(bar.height - 1)
            / f32::from(MAX_GAIN - MIN_GAIN))
        .round() as u16;
        let x = start + bar.width / 2;
        for row in 0..bar.height {
            let symbol = if row == knob {
                "●"
            } else if row == (bar.height - 1) / 2 {
                "┼"
            } else {
                "│"
            };
            frame.render_widget(
                Paragraph::new(symbol).style(style.fg(if row == knob {
                    palette.accent
                } else {
                    palette.separator
                })),
                Rect::new(x, bar.y + row, 1, 1),
            );
        }
        frame.render_widget(
            Paragraph::new(label(app, index))
                .alignment(Alignment::Center)
                .style(style),
            Rect::new(start, bar.bottom(), bar.width, 1),
        );
        app.view.hits.push(Hit {
            area: bar,
            target: Target::EqualizerGain {
                index,
                area: bar,
                vertical: true,
            },
            enabled: true,
        });
        for y in [area.y, bar.bottom()] {
            app.view.hits.push(Hit {
                area: Rect::new(start, y, bar.width, 1),
                target: Target::EqualizerSelect(index),
                enabled: true,
            });
        }
    }
}

fn horizontal(frame: &mut Frame, app: &mut App, selected: usize, area: Rect, palette: Palette) {
    let capacity = usize::from(area.height.div_ceil(2));
    let offset = visible_offset(0, selected, capacity, SLIDERS);
    for index in offset..(offset + capacity).min(SLIDERS) {
        let y = area.y + (index - offset) as u16 * 2;
        if y >= area.bottom() {
            break;
        }
        let style = if index == selected {
            palette.text().bg(palette.selection).fg(palette.accent)
        } else {
            palette.text()
        };
        let gain = app.settings.equalizer.gain(index).unwrap_or(0);
        frame.render_widget(
            Paragraph::new(format!("{} {gain:+} dB", label(app, index))).style(style),
            Rect::new(area.x, y, area.width.min(13), 1),
        );
        app.view.hits.push(Hit {
            area: Rect::new(area.x, y, area.width.min(13), 1),
            target: Target::EqualizerSelect(index),
            enabled: true,
        });
        let bar = Rect::new(area.x + 13, y, area.width.saturating_sub(13), 1);
        if bar.width == 0 {
            continue;
        }
        let knob = (f32::from(gain - MIN_GAIN) * f32::from(bar.width - 1)
            / f32::from(MAX_GAIN - MIN_GAIN))
        .round() as u16;
        for x in 0..bar.width {
            let symbol = if x == knob {
                "●"
            } else if x == (bar.width - 1) / 2 {
                "┼"
            } else {
                "─"
            };
            frame.render_widget(
                Paragraph::new(symbol).style(style),
                Rect::new(bar.x + x, y, 1, 1),
            );
        }
        app.view.hits.push(Hit {
            area: bar,
            target: Target::EqualizerGain {
                index,
                area: bar,
                vertical: false,
            },
            enabled: true,
        });
    }
}

fn dropdown(frame: &mut Frame, app: &mut App, selected: &mut usize, inner: Rect, palette: Palette) {
    // Clicking outside the list dismisses it without touching sliders below.
    app.view.hits.push(Hit {
        area: inner,
        target: Target::EqualizerPresets,
        enabled: true,
    });
    *selected = (*selected).min(PRESETS.len() - 1);
    let outer = Rect::new(
        inner.x + inner.width.saturating_sub(32) / 2,
        inner.y + 2,
        inner.width.min(32),
        inner.height.saturating_sub(3).min(PRESETS.len() as u16 + 2),
    );
    frame.render_widget(Clear, outer);
    let block = block(app.text("Presets", "Пресеты").into(), palette, true);
    let body = block.inner(outer);
    frame.render_widget(block, outer);
    let capacity = usize::from(body.height);
    let offset = visible_offset(0, *selected, capacity, PRESETS.len());
    let current = app.settings.equalizer.preset();
    for (index, preset) in PRESETS.iter().enumerate().skip(offset).take(capacity) {
        let row = Rect::new(body.x, body.y + (index - offset) as u16, body.width, 1);
        let style = if index == *selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                if current == Some(index) { "●" } else { " " },
                preset.name(app.settings.language)
            ))
            .style(style),
            row,
        );
        frame.render_widget(
            Paragraph::new((index + 1).to_string())
                .alignment(Alignment::Right)
                .style(style),
            row,
        );
        app.view.hits.push(Hit {
            area: row,
            target: Target::EqualizerPreset(index),
            enabled: true,
        });
    }
}
