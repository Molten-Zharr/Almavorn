use super::{
    buttons::{button_width, quiet_button},
    theme::Palette,
    widgets::clean,
};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::{RepeatMode, duration_text},
    preferences::{FontFace, PlaybackTimeline},
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
        (
            Action::CycleRepeat,
            match (app.settings.repeat, app.settings.appearance.font) {
                (RepeatMode::Off, FontFace::DejaVuMono) => "[↻]",
                (RepeatMode::Playlist, FontFace::DejaVuMono) => "[↻∞]",
                (RepeatMode::Track, FontFace::DejaVuMono) => "[↻1]",
                (RepeatMode::Off, _) => "[⟳]",
                (RepeatMode::Playlist, _) => "[⟳∞]",
                (RepeatMode::Track, _) => "[⟳1]",
            },
        ),
        (
            Action::ToggleShuffle,
            if app.settings.appearance.font == FontFace::DejaVuMono {
                "[⇄]"
            } else {
                "[↔]"
            },
        ),
        (Action::Equalizer, "[EQ]"),
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
    (width / 3).clamp(32, 80).min(width)
}

fn stack_volume(app: &App, width: u16) -> bool {
    let controls_width: u16 = controls(app)
        .iter()
        .map(|(action, label)| button_width(app, label, &Target::Action(*action)).saturating_add(1))
        .sum();
    width < 70
        || controls_width
            .saturating_add(volume_width(width))
            .saturating_add(15)
            > width
}

fn control_cells(app: &App, width: u16) -> Vec<(Rect, Action, &'static str)> {
    let mut x: u16 = 0;
    let mut y = 0;
    controls(app)
        .into_iter()
        .map(|(action, label)| {
            let size = button_width(app, label, &Target::Action(action)).min(width);
            if x > 0 && x.saturating_add(size) > width {
                x = 0;
                y += 1;
            }
            let rect = Rect::new(x, y, size, 1);
            x = x.saturating_add(size + 1);
            (rect, action, label)
        })
        .collect()
}

fn control_rows(app: &App, width: u16) -> u16 {
    control_cells(app, width)
        .last()
        .map_or(1, |(area, ..)| area.bottom())
}

fn base_height(app: &App, width: u16) -> u16 {
    if stack_volume(app, width) {
        control_rows(app, width).saturating_add(3)
    } else {
        2
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
    let stacked = stack_volume(app, area.width);
    let control_rows = control_rows(app, area.width);
    let volume_width = if stacked {
        area.width
    } else {
        volume_width(area.width)
    };
    let volume_area = Rect::new(
        row.right().saturating_sub(volume_width),
        row.y + if stacked { control_rows } else { 0 },
        volume_width,
        1,
    );
    let mut x = row.x;
    for (cell, action, label) in control_cells(app, area.width) {
        let rect = Rect::new(row.x + cell.x, row.y + cell.y, cell.width, cell.height);
        quiet_button(
            frame,
            app,
            rect,
            label,
            Target::Action(action),
            app.allowed(action),
            palette,
        );
        x = rect.right() + 1;
    }
    volume(frame, app, volume_area, palette);
    let progress = if !stacked {
        Rect::new(x + 1, row.y, volume_area.x.saturating_sub(x + 2), 1)
    } else if area.height >= base_height(app, area.width) {
        Rect::new(area.x, volume_area.bottom(), area.width, 1)
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
    let segments = bar.width;
    let filled = f32::from(percent) * f32::from(segments) / 100.0;
    let blocks = [" ", "▏", "▎", "▍", "▌"];
    let bars: Vec<_> = (0..bar.width)
        .map(|column| {
            let fill = ((filled - f32::from(column)).clamp(0.0, 1.0) * 4.0).round() as usize;
            if fill == 0 {
                Span::styled("▌", palette.text().fg(palette.inactive_panel_border))
            } else {
                Span::styled(blocks[fill], palette.text().fg(palette.accent))
            }
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
