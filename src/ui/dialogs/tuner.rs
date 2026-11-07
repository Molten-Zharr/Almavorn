use super::super::{
    theme::Palette,
    widgets::{buttons, buttons_height, clean, modal, visible_offset},
};
use crate::{
    app::{App, Hit, Target},
    tuner::{SLIDERS, TunerSettings},
};
use ratatui::{Frame, layout::{Margin, Rect}, widgets::{Paragraph, Wrap}};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    selected: &mut usize,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(frame, app, area,
        app.text("Playback tuner", "Настройщик проигрывания").into(),
        area.height, palette).inner(Margin::new(1, 1));
    if inner.is_empty() { return; }
    if inner.width < 24 || inner.height < 16 {
        frame.render_widget(Paragraph::new(app.text(
            "Enlarge the window to adjust playback.", "Увеличьте окно для настройки проигрывания."))
            .style(palette.text()).wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2)));
        buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            vec![(app.text("Close", "Закрыть").into(), Target::CloseDialog, true)], palette);
        return;
    }
    *selected = (*selected).min(SLIDERS - 1);
    let pinned = app.tuner_pinned();
    let enabled = !app.services_playing();
    let scope = if pinned {
        format!("{}: {}", app.text("Only this track", "Только эта композиция"),
            app.playback.current.as_ref().map(|t| clean(&t.title)).unwrap_or_default())
    } else {
        app.text("Shared playback settings", "Общие настройки проигрывания").into()
    };
    frame.render_widget(Paragraph::new(scope).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1));
    let controls = vec![
        (format!("[{}] {}", if pinned { 'x' } else { ' ' },
            app.text("For this track", "Для этой композиции")), Target::TunerPin,
            enabled && app.playback.current.is_some()),
        (format!("[{}] {}", if app.tuner_settings().reverse { 'x' } else { ' ' },
            app.text("Reverse", "Обратное проигрывание")), Target::TunerReverse, enabled),
    ];
    let height = buttons_height(app, inner.width, &controls);
    buttons(frame, app, Rect::new(inner.x, inner.y + 2, inner.width, height), controls, palette);
    let footer_controls = vec![
        (app.text("Reset all", "Сбросить все").into(), Target::TunerReset, enabled),
        (app.text("Close", "Закрыть").into(), Target::CloseDialog, true),
    ];
    let footer_height = buttons_height(app, inner.width, &footer_controls);
    let footer_y = inner.bottom() - footer_height;
    let hint_height = 3;
    let body = Rect::new(inner.x, inner.y + height + 3, inner.width,
        footer_y.saturating_sub(inner.y + height + 4 + hint_height));
    let row_height = if body.height >= SLIDERS as u16 * 4 { 4 } else { 3 };
    let capacity = usize::from(body.height / row_height);
    let offset = visible_offset(0, *selected, capacity, SLIDERS);
    for index in offset..(offset + capacity).min(SLIDERS) {
        let y = body.y + (index - offset) as u16 * row_height;
        let value = app.tuner_settings().value(index).unwrap_or(100);
        let style = if index == *selected {
            palette.text().fg(palette.selection_text).bg(palette.selection)
        } else { palette.text() };
        let label = Rect::new(body.x, y, body.width, 1);
        frame.render_widget(Paragraph::new(format!("{}: {value}%",
            TunerSettings::label(index, app.settings.preferences.language))).style(style), label);
        app.view.hits.push(Hit { area: label, target: Target::TunerSelect(index), enabled });
        // Keep precise controls separate from the draggable bar.
        let control_width = 21.min(body.width);
        let bar = Rect::new(body.x, y + 1, body.width.saturating_sub(control_width + 1), 1);
        if bar.width > 0 {
            let minimum = TunerSettings::minimum(index);
            let maximum = TunerSettings::maximum(index);
            let position = |value: u16| ((f64::from(value - minimum) / f64::from(maximum - minimum))
                * f64::from(bar.width - 1)).round() as u16;
            let knob = position(value);
            let normal = position(TunerSettings::normal(index));
            for x in 0..bar.width {
                frame.render_widget(Paragraph::new(if x == knob { "●" } else if x == normal { "┼" } else { "─" })
                    .style(style.fg(if x == knob { palette.accent } else { palette.separator })),
                    Rect::new(bar.x + x, bar.y, 1, 1));
            }
            app.view.hits.push(Hit { area: bar, target: Target::TunerSlider(index, bar), enabled });
        }
        buttons(frame, app, Rect::new(body.right() - control_width, y + 1, control_width, 1), vec![
            ("-".into(), Target::TunerAdjust(index, -1), enabled && value > TunerSettings::minimum(index)),
            ("+".into(), Target::TunerAdjust(index, 1), enabled && value < TunerSettings::maximum(index)),
            (format!("{}%", TunerSettings::normal(index)), Target::TunerAdjust(index, 0), enabled),
        ], palette);
        if row_height == 4 {
            frame.render_widget(Paragraph::new(TunerSettings::description(index, app.settings.preferences.language))
                .style(palette.text().fg(palette.muted)), Rect::new(body.x, y + 2, body.width, 1));
        }
    }
    let hint = if !enabled {
        app.text("Playback settings are available for local audio files.",
            "Настройки проигрывания доступны для локальных аудиофайлов.").into()
    } else {
        format!("{}\n{}", TunerSettings::description(*selected, app.settings.preferences.language),
            app.text("Changes apply immediately. Arrows: select/adjust; Home: reset parameter.",
                "Изменения слышны сразу. Стрелки: выбор и шаг; Home: сброс параметра."))
    };
    frame.render_widget(Paragraph::new(hint).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false }),
        Rect::new(inner.x, footer_y - hint_height - 1, inner.width, hint_height));
    buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height), footer_controls, palette);
}
