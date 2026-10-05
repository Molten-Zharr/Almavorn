use super::{theme::Palette, toolbar::Group, widgets::clean};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::duration_text,
    workspace::Panel,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Gauge, Paragraph},
};

pub(super) fn controls(app: &App) -> Vec<Group> {
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
    let mut group = Group {
        panel: Panel::Player,
        title: app.text("Player", "Проигрыватель").into(),
        entries: playback
            .iter()
            .chain(volume.iter())
            .map(|(action, en, ru)| {
                (
                    app.text(en, ru).into(),
                    Target::Action(*action),
                    app.allowed(*action),
                )
            })
            .collect(),
    };
    group.retain_visible(app);
    vec![group]
}

pub(super) fn preferred_height(app: &App, width: u16) -> u16 {
    let groups = controls(app);
    content_height(app, width.saturating_sub(2), &groups[0]).saturating_add(2)
}

pub(super) fn content_height(app: &App, width: u16, controls: &Group) -> u16 {
    let rows = if controls.entries.is_empty() {
        0
    } else {
        super::toolbar::rows(app, width, controls)
    };
    rows.saturating_add(4)
}

pub(super) fn player(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    controls: &Group,
    palette: Palette,
) {
    let inner = area;
    if inner.height < 4 {
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
    let rows = content_height(app, inner.width, controls).saturating_sub(4);
    super::toolbar::contents(
        frame,
        app,
        Rect::new(inner.x, inner.y + 1, inner.width, rows),
        controls,
        palette,
    );
    let y = inner.y + rows + 2;
    if y >= inner.bottom() {
        return;
    }
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
