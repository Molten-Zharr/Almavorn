use super::{
    theme::Palette,
    widgets::{clean, visible_offset},
};
use crate::{
    app::{App, Hit, Sort, Target},
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table, Wrap},
};

pub(super) fn playlists(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.playlist_area = area;
    let inner = area;
    let items: Vec<(i64, String, usize)> = app
        .visible_playlists()
        .iter()
        .map(|playlist| {
            (
                playlist.id,
                playlist.display_name(app.settings.language).to_owned(),
                playlist.entries.len(),
            )
        })
        .collect();
    let selected = items
        .iter()
        .position(|(id, _, _)| Some(*id) == app.selected_playlist)
        .unwrap_or(0);
    app.playlist_offset = visible_offset(
        app.playlist_offset,
        selected,
        inner.height as usize,
        items.len(),
    );
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text("Create a playlist", "Создайте плейлист"))
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
    for (row, (id, name, count)) in items
        .iter()
        .skip(app.playlist_offset)
        .take(inner.height as usize)
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let selected = Some(*id) == app.selected_playlist;
        let style = if selected {
            palette
                .text()
                .bg(palette.sidebar_selection)
                .fg(palette.background)
        } else {
            palette.text()
        };
        let playing = app.playback_active() && app.playing_playlist == Some(*id);
        let mut content = Vec::new();
        if playing {
            content.push(Span::styled(
                format!(
                    "[{}] ",
                    if app.current_paused() {
                        app.text("PAUSED", "ПАУЗА")
                    } else {
                        app.text("PLAYING", "ИГРАЕТ")
                    }
                ),
                style
                    .bg(if app.current_paused() {
                        palette.muted
                    } else {
                        palette.accent
                    })
                    .fg(palette.background)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            content.push(Span::styled("  ", style));
        }
        content.push(Span::styled(format!("{name} · {count}"), style));
        frame.render_widget(Paragraph::new(Line::from(content)).style(style), rect);
        app.hits.push(Hit {
            area: rect,
            target: Target::Playlist(*id),
            enabled: true,
        });
    }
}

pub(super) fn tracks_table(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.tracks_area = area;
    let inner = area;
    if inner.height < 2 {
        return;
    }
    let name = app
        .playlist()
        .map(|playlist| playlist.display_name(app.settings.language))
        .unwrap_or(app.text("Tracks", "Композиции"));
    let title = format!(
        "{name} · {} {}{}",
        app.sort.name(app.settings.language),
        if app.sort_descending { "↓" } else { "↑" },
        if app.query.is_empty() {
            String::new()
        } else {
            format!(" · {}", app.query)
        }
    );
    frame.render_widget(
        Paragraph::new(title).style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let inner = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
    let capacity = inner.height.saturating_sub(1) as usize;
    let entries = app.rows();
    let selected = entries
        .iter()
        .position(|entry| Some(entry.id) == app.selected_entry)
        .unwrap_or(0);
    let offset = visible_offset(app.track_offset, selected, capacity, entries.len());
    let ids: Vec<i64> = entries
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|entry| entry.id)
        .collect();
    let rows: Vec<_> = entries
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|entry| {
            let selected = Some(entry.id) == app.selected_entry;
            let playing = app.playback_active()
                && app.playing_playlist == app.selected_playlist
                && app.playing_entry == Some(entry.id);
            let mark = if app.marked.contains(&entry.id) {
                "[x]"
            } else {
                "[ ]"
            };
            let style = if selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            };
            let mut title = Vec::new();
            if playing {
                title.push(Span::styled(
                    format!(
                        "[{}] ",
                        if app.current_paused() {
                            app.text("PAUSED", "ПАУЗА")
                        } else {
                            app.text("PLAYING", "ИГРАЕТ")
                        }
                    ),
                    style
                        .bg(if app.current_paused() {
                            palette.muted
                        } else {
                            palette.accent
                        })
                        .fg(palette.background)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            title.push(Span::styled(clean(&entry.track.title), style));
            Row::new(vec![
                Cell::from(mark),
                Cell::from((entry.position + 1).to_string()),
                Cell::from(Line::from(title)),
                Cell::from(clean(&entry.track.artist)),
                Cell::from(duration_text(entry.track.duration_ms)),
            ])
            .style(style)
        })
        .collect();
    let empty = entries.is_empty();
    app.track_offset = offset;
    let columns = if inner.width < 55 {
        vec![
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(0),
            Constraint::Length(7),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Percentage(52),
            Constraint::Min(8),
            Constraint::Length(8),
        ]
    };
    let column_areas = Layout::horizontal(columns.clone())
        .spacing(1)
        .flex(Flex::Start)
        .split(Rect::new(inner.x, inner.y, inner.width, 1));
    let labels = [
        "",
        "#",
        app.text("Title", "Название"),
        app.text("Artist", "Исполнитель"),
        app.text("Time", "Время"),
    ];
    let sorts = [
        None,
        Some(Sort::Position),
        Some(Sort::Title),
        Some(Sort::Artist),
        Some(Sort::Duration),
    ];
    let mut header_cells = Vec::new();
    for (index, (label, sort)) in labels.into_iter().zip(sorts).enumerate() {
        let rect = column_areas[index];
        let active = sort == Some(app.sort);
        let mut style = palette.text().fg(if active {
            palette.accent
        } else {
            palette.muted
        });
        if sort.is_some() && app.hovered(rect) {
            style = style.bg(palette.selection).add_modifier(Modifier::BOLD);
        }
        let label = if active {
            format!("{} {label}", if app.sort_descending { "↓" } else { "↑" })
        } else {
            label.to_owned()
        };
        header_cells.push(Cell::from(label).style(style));
        if let Some(sort) = sort
            && rect.width > 0
        {
            app.hits.push(Hit {
                area: rect,
                target: Target::SortColumn(sort),
                enabled: true,
            });
        }
    }
    let header = Row::new(header_cells).style(palette.text().fg(palette.muted));
    frame.render_widget(
        Table::new(rows, columns)
            .header(header)
            .column_spacing(1)
            .flex(Flex::Start),
        inner,
    );
    if empty {
        frame.render_widget(
            Paragraph::new(app.text(
                "Add files, a folder, or copy tracks from the sorting desk.",
                "Добавьте файлы, папку или скопируйте композиции из сортировочного стола.",
            ))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y + 2,
                inner.width,
                inner.height.saturating_sub(2),
            ),
        );
    }
    for (row, id) in ids.into_iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1);
        app.hits.push(Hit {
            area: rect,
            target: Target::Track(id),
            enabled: true,
        });
        app.hits.push(Hit {
            area: Rect::new(rect.x, rect.y, 3.min(rect.width), 1),
            target: Target::Mark(id),
            enabled: true,
        });
    }
}
