use super::{
    buttons::{quiet_button, quiet_width},
    theme::Palette,
    widgets::clean,
};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::duration_text,
    preferences::PlaybackTimeline,
    workspace::Panel,
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Sparkline},
};

fn controls(app: &App) -> Vec<(Action, &'static str)> {
    [
        (Action::Previous, "‹"),
        (Action::TogglePlay, if app.paused() { "[▶]" } else { "[‖]" }),
        (Action::Stop, "[■]"),
        (Action::Next, "›"),
    ]
    .into_iter()
    .filter(|(action, _)| app.control_visible(Panel::Player, *action))
    .collect()
}

pub(super) fn preferred_height(app: &App, width: u16) -> u16 {
    content_height(app, width.saturating_sub(2)) + 2
}

pub(super) fn content_height(app: &App, width: u16) -> u16 {
    base_height(app, width)
        + if app.playback_active()
            && app.settings.appearance.playback_timeline == PlaybackTimeline::Waveform
        {
            4
        } else {
            0
        }
}

fn volume_width(width: u16) -> u16 {
    if width >= 70 {
        22
    } else {
        (width / 2).clamp(8, 22)
    }
}

fn stack_volume(app: &App, width: u16) -> bool {
    let controls_width: u16 = controls(app)
        .iter()
        .map(|(_, label)| quiet_width(label).saturating_add(1))
        .sum();
    controls_width.saturating_add(volume_width(width)) > width
}

fn base_height(app: &App, width: u16) -> u16 {
    if stack_volume(app, width) {
        4
    } else if width >= 70 {
        2
    } else {
        3
    }
}

pub(super) fn player(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if area.height < 2 || area.width < 8 {
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
        app.text(
            "Choose a track to play",
            "Выберите композицию для воспроизведения",
        )
        .to_owned()
    };
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(if app.playback_active() {
            palette.accent
        } else {
            palette.muted
        })),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let row = Rect::new(area.x, area.y + 1, area.width, 1);
    let volume_width = volume_width(area.width).min(area.width);
    let stacked = stack_volume(app, area.width);
    let volume_area = Rect::new(
        row.right().saturating_sub(volume_width),
        row.y + u16::from(stacked),
        volume_width,
        1,
    );
    let mut x = row.x;
    for (action, label) in controls(app) {
        let width = quiet_width(label);
        if x + width > if stacked { row.right() } else { volume_area.x } {
            break;
        }
        quiet_button(
            frame,
            app,
            Rect::new(x, row.y, width, 1),
            label,
            Target::Action(action),
            app.allowed(action),
            palette,
        );
        x += width + 1;
    }
    volume(frame, app, volume_area, palette);
    let progress = if area.width >= 70 && !stacked {
        Rect::new(x + 1, row.y, volume_area.x.saturating_sub(x + 2), 1)
    } else if area.height >= base_height(app, area.width) {
        Rect::new(area.x, area.y + 2 + u16::from(stacked), area.width, 1)
    } else {
        Rect::default()
    };
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
    let time = format!("{} / {}", duration_text(position), duration_text(duration));
    let time_width = (Span::raw(&time).width() as u16).min(progress.width);
    let seek = Rect::new(
        progress.x,
        progress.y,
        progress.width.saturating_sub(time_width + 1),
        1,
    );
    if app.settings.appearance.playback_timeline == PlaybackTimeline::Progress {
        app.view.progress_area = seek;
        slider(frame, seek, ratio, palette);
        app.view.hits.push(Hit {
            area: seek,
            target: Target::Seek(seek),
            enabled: app.playback_active() && duration > 0 && seek.width > 0,
        });
    }
    frame.render_widget(
        Paragraph::new(time).style(palette.text().fg(palette.muted)),
        Rect::new(
            progress.right().saturating_sub(time_width),
            progress.y,
            time_width,
            1,
        ),
    );
    let wave_y = area.y + base_height(app, area.width);
    if app.playback_active()
        && app.settings.appearance.playback_timeline == PlaybackTimeline::Waveform
        && wave_y + 4 <= area.bottom()
    {
        let wave = Rect::new(area.x, wave_y, area.width, 4);
        app.view.waveform_area = wave;
        waveform(frame, app, wave, ratio, palette);
        app.view.hits.push(Hit {
            area: wave,
            target: Target::Seek(wave),
            enabled: duration > 0,
        });
    }
}

fn slider(frame: &mut Frame, area: Rect, ratio: f64, palette: Palette) {
    if area.width == 0 {
        return;
    }
    let filled = (ratio * f64::from(area.width.saturating_sub(1))).round() as u16;
    let spans: Vec<_> = (0..area.width)
        .map(|index| {
            Span::styled(
                if index == filled { "●" } else { "─" },
                palette.text().fg(if index <= filled && ratio > 0.0 {
                    palette.accent
                } else {
                    palette.separator
                }),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn waveform(frame: &mut Frame, app: &App, area: Rect, ratio: f64, palette: Palette) {
    let Some(values) = app
        .playback
        .waveform
        .as_deref()
        .filter(|values| !values.is_empty())
    else {
        let text = if app.playback.current.is_none() {
            "─".repeat(usize::from(area.width))
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
    let percent = (app.settings.volume.clamp(0.0, 1.0) * 100.0).round() as u16;
    let label = if area.width >= 20 {
        app.text("Vol", "Громк")
    } else {
        ""
    };
    let label_width = Span::raw(label).width() as u16 + u16::from(!label.is_empty());
    frame.render_widget(
        Paragraph::new(label).style(palette.text().fg(palette.muted)),
        Rect::new(area.x, area.y, label_width, 1),
    );
    let bar = Rect::new(
        area.x + label_width,
        area.y,
        area.width.saturating_sub(label_width + 5),
        1,
    );
    let segments = bar.width.div_ceil(2);
    let filled = (f32::from(percent) * f32::from(segments) / 100.0).round() as u16;
    let bars: Vec<_> = (0..bar.width)
        .map(|column| {
            Span::styled(
                if column % 2 == 0 { "█" } else { " " },
                palette.text().fg(if column / 2 < filled {
                    palette.accent
                } else {
                    palette.inactive_panel_border
                }),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(bars)), bar);
    frame.render_widget(
        Paragraph::new(format!("{percent:>3}%")).style(palette.text().fg(palette.muted)),
        Rect::new(area.right().saturating_sub(4), area.y, 4, 1),
    );
    app.view.hits.push(Hit {
        area: bar,
        target: Target::PlaybackVolume(bar),
        enabled: bar.width > 0,
    });
}
