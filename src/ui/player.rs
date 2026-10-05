use super::{
    theme::Palette,
    toolbar::{self, Group},
    widgets::{block, clean},
};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Gauge, Paragraph},
};

fn controls(app: &App) -> Vec<Group> {
    let playback = [
        (Action::Previous, "<<", "<<"),
        (
            Action::TogglePlay,
            if app.paused() { "Play" } else { "Pause" },
            if app.paused() {
                "Играть"
            } else {
                "Пауза"
            },
        ),
        (Action::Stop, "Stop", "Стоп"),
        (Action::Next, ">>", ">>"),
    ];
    let volume = [
        (Action::VolumeDown, "Vol -", "Громк -"),
        (Action::VolumeUp, "Vol +", "Громк +"),
    ];
    [
        (app.text("Playback", "Управление"), playback.as_slice()),
        (app.text("Volume", "Громкость"), volume.as_slice()),
    ]
    .into_iter()
    .map(|(title, actions)| Group {
        title: title.into(),
        entries: actions
            .iter()
            .map(|(action, en, ru)| {
                (
                    app.text(en, ru).into(),
                    Target::Action(*action),
                    app.allowed(*action),
                )
            })
            .collect(),
    })
    .collect()
}

pub(super) fn preferred_height(app: &App, width: u16) -> u16 {
    toolbar::height(app, &controls(app), width.saturating_sub(2), true).saturating_add(4)
}

pub(super) fn player(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let outer = block(app.text("Player", "Проигрыватель").into(), palette, false);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 3 {
        return;
    }
    let title = app
        .current
        .as_ref()
        .map(|track| format!("{} — {}", track.title, track.artist))
        .unwrap_or_else(|| {
            app.text("No track playing", "Ничего не воспроизводится")
                .into()
        });
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let groups = controls(app);
    let used = toolbar::height(app, &groups, inner.width, true).min(inner.height.saturating_sub(2));
    toolbar::render(
        frame,
        app,
        Rect::new(inner.x, inner.y + 1, inner.width, used),
        groups,
        true,
        palette,
    );
    let y = (inner.y + 1 + used).min(inner.bottom().saturating_sub(1));
    let duration = app.current.as_ref().map_or(0, |track| track.duration_ms);
    let position = app.position_ms();
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rect = Rect::new(inner.x, y, inner.width, 1);
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .gauge_style(Style::default().fg(palette.accent).bg(palette.selection))
            .label(format!(
                "{} / {} · {}%",
                duration_text(position),
                duration_text(duration),
                (app.settings.volume * 100.0).round()
            )),
        rect,
    );
    app.hits.push(Hit {
        area: rect,
        target: Target::Seek(rect),
        enabled: duration > 0,
    });
}
