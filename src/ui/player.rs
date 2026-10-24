use super::{
    buttons::{button_width, quiet_button},
    theme::Palette,
    widgets::clean,
};
use crate::layout::{player_metrics, player_target};
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
    style::Color,
    text::{Line, Span},
    widgets::{Paragraph, Sparkline},
};

pub(super) fn player(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if area.height < 2 || area.width < 8 {
        return;
    }
    let metrics = player_metrics(app);
    let geometry = if app.settings.preferences.artwork.visible && app.playback.current.is_some() {
        metrics.arrange_with_artwork(area, app.settings.preferences.artwork.size.columns())
    } else {
        metrics.arrange(area)
    };
    if let Some(cover) = geometry.artwork {
        app.view.artwork_area = cover;
        artwork(frame, app, cover, palette);
        app.view.hits.push(Hit {
            area: cover,
            target: Target::Artwork,
            enabled: app.artwork_image().is_some() && !app.opening_artwork(),
        });
    }
    let area = geometry.header;
    let mut title = if app.preparing_playback() && !app.playback_active() {
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
    let mut title_width = area.width;
    let loop_rows = geometry.loop_rows;
    if let Some(count) = app.marked_playback_count() {
        let caption = format!("{}: {count}", app.text("Checked loop", "Повтор отметок"));
        let mut caption_width = area.width;
        if app.control_visible(Panel::Player, Action::PlayWholePlaylist) {
            let label = app.text("All tracks", "Все треки");
            let target = Target::Action(Action::PlayWholePlaylist);
            let width = button_width(app, label, &target).min(area.width.saturating_sub(8));
            caption_width = area.width.saturating_sub(width + 1);
            quiet_button(
                frame,
                app,
                Rect::new(area.right() - width, area.y + loop_rows, width, 1),
                label,
                target,
                true,
                palette,
            );
        }
        if loop_rows > 0 {
            frame.render_widget(
                Paragraph::new(caption).style(palette.text().fg(palette.accent)),
                Rect::new(area.x, area.y + loop_rows, caption_width, 1),
            );
        } else {
            title = format!("{caption} · {title}");
            title_width = caption_width;
        }
    }
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(if app.playback_active() {
            palette.accent
        } else {
            palette.muted
        })),
        Rect::new(area.x, area.y, title_width, 1),
    );
    app.view.hits.push(Hit {
        area: Rect::new(area.x, area.y, title_width, 1),
        target: Target::PlayingTrack,
        enabled: app.playback.current.is_some() && !app.busy(),
    });
    for (rect, action, label) in geometry.controls {
        quiet_button(
            frame,
            app,
            rect,
            label,
            player_target(action),
            app.allowed(action),
            palette,
        );
    }
    volume(frame, app, geometry.volume, palette);
    let progress = geometry.progress;
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
    if app.settings.preferences.appearance.playback_timeline == PlaybackTimeline::Progress {
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
    if let Some(wave) = geometry.waveform {
        app.view.waveform_area = wave;
        waveform(frame, app, wave, ratio, palette);
        app.view.hits.push(Hit {
            area: wave,
            target: Target::Seek(wave),
            enabled: duration > 0,
        });
    }
}

fn artwork(frame: &mut Frame, app: &App, area: Rect, palette: Palette) {
    let Some(image) = app.artwork_image() else {
        frame.render_widget(
            Paragraph::new("♪")
                .alignment(Alignment::Center)
                .style(palette.text().fg(palette.accent)),
            Rect::new(area.x, area.y + area.height / 2, area.width, 1),
        );
        return;
    };
    let background = match palette.background {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => [0; 3],
    };
    let scale = (f32::from(area.width) / image.width as f32)
        .min(f32::from(area.height * 2) / image.height as f32);
    let width = (image.width as f32 * scale).round().max(1.0) as u16;
    let height = (image.height as f32 * scale).round().max(1.0) as u16;
    let left = (area.width - width) / 2;
    let top = (area.height * 2 - height) / 2;
    let sample = |x: u16, y: u16| {
        if x < left || y < top || x >= left + width || y >= top + height {
            return Color::Rgb(background[0], background[1], background[2]);
        }
        let px = u32::from(x - left) * image.width / u32::from(width);
        let py = u32::from(y - top) * image.height / u32::from(height);
        let pixel = ((py * image.width + px) * 4) as usize;
        let alpha = u32::from(image.rgba[pixel + 3]);
        let rgb: [u8; 3] = std::array::from_fn(|channel| {
            ((u32::from(image.rgba[pixel + channel]) * alpha
                + u32::from(background[channel]) * (255 - alpha))
                / 255) as u8
        });
        Color::Rgb(rgb[0], rgb[1], rgb[2])
    };
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = frame.buffer_mut().cell_mut((area.x + x, area.y + y)) {
                cell.set_symbol("▀")
                    .set_fg(sample(x, y * 2))
                    .set_bg(sample(x, y * 2 + 1));
            }
        }
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
    let percent = (app.settings.preferences.volume.clamp(0.0, 1.0) * 100.0).round() as u16;
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
