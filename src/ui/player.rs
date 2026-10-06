use super::{theme::Palette, toolbar::Group, widgets::clean};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::duration_text,
    preferences::PlaybackTimeline,
    workspace::Panel,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Gauge, Paragraph, Sparkline},
};

pub(super) fn controls(app: &App) -> Vec<Group> {
    let playback = [
        (Action::Previous, "<<", "<<"),
        (
            Action::TogglePlay,
            if app.paused() { "[▶]" } else { "[‖]" },
            if app.paused() { "[▶]" } else { "[‖]" },
        ),
        (Action::Stop, "[■]", "[■]"),
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
    rows.saturating_add(4 + timeline_height(app))
}

fn timeline_height(app: &App) -> u16 {
    match app.settings.appearance.playback_timeline {
        PlaybackTimeline::Waveform => 4,
        PlaybackTimeline::Progress => 1,
    }
}

pub(super) fn player(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    controls: &Group,
    palette: Palette,
) {
    let inner = area;
    if inner.height < content_height(app, inner.width, controls) {
        return;
    }
    let title = if app.preparing_playback() && !app.playback_active() {
        app.text("Preparing audio…", "Подготовка аудио…").to_owned()
    } else if let Some(track) = app
        .playback
        .current
        .as_ref()
        .filter(|_| app.playback_active())
    {
        format!(
            "[{}] {}{}",
            if app.current_paused() { "‖" } else { "▶" },
            track.title,
            if track.artist.is_empty() {
                String::new()
            } else {
                format!(" — {}", track.artist)
            }
        )
    } else {
        app.text("No track playing", "Ничего не воспроизводится")
            .to_owned()
    };
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let rows = super::toolbar::rows(app, inner.width, controls);
    super::toolbar::contents(
        frame,
        app,
        Rect::new(inner.x, inner.y + 1, inner.width, rows),
        controls,
        palette,
    );
    let y = inner.y + rows + 2;
    let duration = app
        .playback
        .current
        .as_ref()
        .map_or(0, |track| track.duration_ms);
    let position = app.position_ms();
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let seek = Rect::new(inner.x, y, inner.width, timeline_height(app));
    match app.settings.appearance.playback_timeline {
        PlaybackTimeline::Waveform => {
            app.view.waveform_area = seek;
            waveform(frame, app, seek, ratio, palette);
        }
        PlaybackTimeline::Progress => {
            app.view.progress_area = seek;
            frame.render_widget(
                Gauge::default()
                    .ratio(ratio)
                    .gauge_style(
                        Style::default()
                            .fg(palette.accent)
                            .bg(palette.inactive_panel_border),
                    )
                    .use_unicode(true)
                    .label(""),
                seek,
            );
        }
    }
    frame.render_widget(
        Paragraph::new(format!(
            "{} / {}",
            duration_text(position),
            duration_text(duration)
        ))
        .alignment(Alignment::Center)
        .style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, seek.bottom(), inner.width, 1),
    );
    app.view.hits.push(Hit {
        area: seek,
        target: Target::Seek(seek),
        enabled: app.playback_active() && duration > 0,
    });
    volume(
        frame,
        app,
        Rect::new(inner.x, seek.bottom() + 1, inner.width, 1),
        palette,
    );
}

fn waveform(frame: &mut Frame, app: &App, area: Rect, ratio: f64, palette: Palette) {
    let Some(values) = app
        .playback
        .waveform
        .as_deref()
        .filter(|values| !values.is_empty())
    else {
        let text = if app.playback.current.is_none() {
            String::new()
        } else if app.preparing_waveform() || app.preparing_playback() {
            app.text("Building waveform…", "Обработка аудиоволны…")
                .to_owned()
        } else {
            app.text("Waveform unavailable", "Аудиоволна недоступна")
                .to_owned()
        };
        frame.render_widget(
            Paragraph::new(text).style(palette.text().fg(palette.inactive_panel_border)),
            area,
        );
        return;
    };
    let peak = values
        .iter()
        .copied()
        .fold(0.0f32, f32::max)
        .max(f32::EPSILON);
    let width = usize::from(area.width.max(1));
    let data: Vec<u64> = (0..width)
        .map(|column| {
            let start = column * values.len() / width;
            let end = ((column + 1) * values.len() / width)
                .max(start + 1)
                .min(values.len());
            let value = values[start..end].iter().copied().fold(0.0f32, f32::max);
            (f64::from(value / peak).clamp(0.0, 1.0) * 1000.0).round() as u64
        })
        .collect();
    frame.render_widget(
        Sparkline::default()
            .data(&data)
            .max(1000)
            .style(palette.text().fg(palette.inactive_panel_border)),
        area,
    );
    let played = (ratio * f64::from(area.width)).floor() as u16;
    for x in area.x..area.x.saturating_add(played).min(area.right()) {
        for y in area.y..area.bottom() {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_fg(palette.accent);
            }
        }
    }
}

fn volume(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let label = app.text("Volume", "Громкость");
    let label_width = Span::raw(label).width() as u16;
    let segments = (area.width.saturating_sub(label_width + 7) / 2).clamp(1, 30);
    let bar_width = segments * 2 - 1;
    let total = label_width + bar_width + 7;
    let start = area.x + area.width.saturating_sub(total);
    let bar = Rect::new(start + label_width + 2, area.y, bar_width, 1).intersection(area);
    let percent = (app.settings.volume.clamp(0.0, 1.0) * 100.0).round() as u16;
    let filled = (f32::from(percent) * f32::from(segments) / 100.0).round() as u16;
    frame.render_widget(
        Paragraph::new(label).style(palette.text().fg(palette.muted)),
        Rect::new(start, area.y, label_width, 1).intersection(area),
    );
    let mut content = Vec::new();
    for segment in 0..segments {
        if segment > 0 {
            content.push(Span::styled(" ", palette.text()));
        }
        content.push(Span::styled(
            "█",
            palette.text().fg(if segment < filled {
                palette.accent
            } else {
                palette.inactive_panel_border
            }),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(content)).style(palette.text()),
        bar,
    );
    frame.render_widget(
        Paragraph::new(format!("{percent:>3}%")).style(palette.text().fg(palette.accent)),
        Rect::new(bar.right() + 1, area.y, 4, 1).intersection(area),
    );
    app.view.hits.push(Hit {
        area: bar,
        target: Target::PlaybackVolume(bar),
        enabled: bar.width > 0,
    });
}
