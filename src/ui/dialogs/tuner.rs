use super::super::{
    theme::Palette,
    widgets::{buttons, modal, visible_offset},
};
use crate::{
    app::{App, Hit, Target},
    tuner::{SLIDERS, TunerSettings},
};
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    selected: &mut usize,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Playback tuner", "Настройщик проигрывания").into(),
        27,
        palette,
    );
    if inner.height < 6 || inner.width < 8 {
        return;
    }
    *selected = (*selected).min(SLIDERS - 1);
    let header_height = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y, inner.width, 3),
        vec![
            (
                format!(
                    "[{}] {}",
                    if app.settings.tuner.reverse { 'x' } else { ' ' },
                    if inner.width < 48 {
                        app.text("Reverse", "Реверс")
                    } else {
                        app.text("Reverse", "Обратное проигрывание")
                    }
                ),
                Target::TunerReverse,
                true,
            ),
            (app.text("Reset", "Сброс").into(), Target::TunerReset, true),
        ],
        palette,
    );
    let body = Rect::new(
        inner.x,
        inner.y + header_height + 1,
        inner.width,
        inner.height.saturating_sub(header_height + 4),
    );
    let compact = body.width < 48 || body.height < 16;
    let row_height = if compact { 2 } else { 4 };
    let capacity = usize::from(body.height / row_height).max(1);
    let offset = visible_offset(0, *selected, capacity, SLIDERS);
    for index in offset..(offset + capacity).min(SLIDERS) {
        let y = body.y + (index - offset) as u16 * row_height;
        if y + row_height > body.bottom() {
            break;
        }
        let value = app.settings.tuner.value(index).unwrap_or(100);
        let detail = if index == 2 && !compact {
            format!(
                " ({:+.1} {})",
                12.0 * (f64::from(value) / 100.0).log2(),
                app.text("semitones", "полутонов")
            )
        } else {
            String::new()
        };
        let style = if index == *selected {
            palette.text().fg(palette.accent).bg(palette.selection)
        } else {
            palette.text()
        };
        let label_area = Rect::new(body.x, y, body.width, 1);
        frame.render_widget(
            Paragraph::new(format!(
                "{}: {value}%{detail}",
                TunerSettings::label(index, app.settings.language)
            ))
            .style(style),
            label_area,
        );
        app.view.hits.push(Hit {
            area: label_area,
            target: Target::TunerSelect(index),
            enabled: true,
        });
        let bar = Rect::new(body.x + 1, y + 1, body.width.saturating_sub(2), 1);
        if bar.width > 0 {
            let minimum = TunerSettings::minimum(index);
            let maximum = TunerSettings::maximum(index);
            let knob = (f64::from(value - minimum) * f64::from(bar.width - 1)
                / f64::from(maximum - minimum))
            .round() as u16;
            let normal = (f64::from(TunerSettings::normal(index) - minimum)
                * f64::from(bar.width - 1)
                / f64::from(maximum - minimum))
            .round() as u16;
            for x in 0..bar.width {
                let symbol = if x == knob {
                    "●"
                } else if x == normal {
                    "┼"
                } else {
                    "─"
                };
                frame.render_widget(
                    Paragraph::new(symbol).style(style.fg(if x == knob {
                        palette.accent
                    } else {
                        palette.separator
                    })),
                    Rect::new(bar.x + x, bar.y, 1, 1),
                );
            }
            app.view.hits.push(Hit {
                area: bar,
                target: Target::TunerSlider(index, bar),
                enabled: true,
            });
        }
        if !compact {
            frame.render_widget(
                Paragraph::new(TunerSettings::description(index, app.settings.language))
                    .style(palette.text().fg(palette.muted)),
                Rect::new(body.x, y + 2, body.width, 1),
            );
        }
    }
    frame.render_widget(
        Paragraph::new(app.text(
            "↑↓: select · ←→/wheel: 1% · Home: normal",
            "↑↓: выбор · ←→/колесо: 1% · Home: норма",
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
}
