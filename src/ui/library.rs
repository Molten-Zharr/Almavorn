use super::{
    theme::Palette,
    widgets::{clean, visible_offset},
};
use crate::{
    app::{App, Hit, Target},
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    widgets::{Paragraph, Row, Table, Wrap},
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
        frame.render_widget(
            Paragraph::new(format!(
                "{} {name} · {count}",
                if selected { ">" } else { " " }
            ))
            .style(if selected {
                palette
                    .text()
                    .bg(palette.sidebar_selection)
                    .fg(palette.background)
            } else {
                palette.text()
            }),
            rect,
        );
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
        "{name} · {}{}",
        app.sort.name(app.settings.language),
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
            let playing = app
                .current
                .as_ref()
                .is_some_and(|track| track.id == entry.track.id);
            let mark = if app.marked.contains(&entry.id) {
                "[x]"
            } else {
                "[ ]"
            };
            Row::new(vec![
                mark.to_owned(),
                format!("{}{}", if playing { ">" } else { "" }, entry.position + 1),
                clean(&entry.track.title),
                clean(&entry.track.artist),
                duration_text(entry.track.duration_ms),
            ])
            .style(if selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            })
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
            Constraint::Length(5),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Percentage(52),
            Constraint::Min(8),
            Constraint::Length(6),
        ]
    };
    let header = Row::new(vec![
        "".to_owned(),
        "#".to_owned(),
        app.text("Title", "Название").to_owned(),
        app.text("Artist", "Исполнитель").to_owned(),
        app.text("Time", "Время").to_owned(),
    ])
    .style(palette.text().fg(palette.muted));
    frame.render_widget(
        Table::new(rows, columns).header(header).column_spacing(1),
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
