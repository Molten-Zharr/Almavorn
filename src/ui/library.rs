use super::{
    theme::Palette,
    widgets::{block, button, clean, visible_offset},
};
use crate::{
    app::{App, Hit, Sort, Target},
    model::duration_text,
    workspace::Gesture,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Clear, Paragraph, Row, Table, Wrap},
};

pub(super) fn playlists(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.view.playlist_area = area;
    let inner = area;
    let items: Vec<(i64, String, usize)> = app
        .visible_playlists()
        .iter()
        .map(|playlist| {
            (
                playlist.id,
                clean(playlist.display_name(app.settings.language)),
                playlist.entries.len(),
            )
        })
        .collect();
    let selected = items
        .iter()
        .position(|(id, _, _)| Some(*id) == app.library.selected_playlist)
        .unwrap_or(0);
    let dragging = matches!(app.view.workspace.gesture, Some(Gesture::Playlist { .. }));
    app.view.playlist_offset = if dragging {
        app.view
            .playlist_offset
            .min(items.len().saturating_sub(usize::from(inner.height)))
    } else {
        visible_offset(
            app.view.playlist_offset,
            selected,
            inner.height as usize,
            items.len(),
        )
    };
    let destination = match &app.view.workspace.gesture {
        Some(Gesture::Playlist { target, .. }) => *target,
        _ => None,
    };
    let position_width = items.len().max(1).to_string().len();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text("Create a playlist", "Создайте плейлист"))
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
    for (index, (id, name, count)) in items
        .iter()
        .enumerate()
        .skip(app.view.playlist_offset)
        .take(inner.height as usize)
    {
        let row = index - app.view.playlist_offset;
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let selected = Some(*id) == app.library.selected_playlist;
        let style = if selected {
            palette
                .text()
                .bg(palette.sidebar_selection)
                .fg(palette.background)
        } else if destination == Some(*id) {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        let playing = app.playback_active() && app.playback.playing_playlist == Some(*id);
        let mut content = vec![Span::styled(
            format!("{:>position_width$}. ", index + 1),
            style
                .fg(if selected {
                    palette.background
                } else {
                    palette.accent
                })
                .add_modifier(Modifier::BOLD),
        )];
        if playing {
            content.push(Span::styled(
                format!("[{}]", if app.current_paused() { "‖" } else { "▶" }),
                style
                    .bg(if app.current_paused() {
                        palette.muted
                    } else {
                        palette.accent
                    })
                    .fg(palette.background)
                    .add_modifier(Modifier::BOLD),
            ));
            content.push(Span::styled(" ", style));
        }
        let count = count.to_string();
        let counter = if usize::from(rect.width) > count.len() + 3 {
            format!(" · {count}")
        } else {
            count
        };
        let count_width = (Span::raw(counter.as_str()).width() as u16).min(rect.width);
        let name_area = Rect::new(rect.x, rect.y, rect.width - count_width, 1);
        let prefix_width: usize = content.iter().map(Span::width).sum();
        content.push(Span::styled(
            ellipsize_name(
                name,
                usize::from(name_area.width).saturating_sub(prefix_width),
            ),
            style,
        ));
        frame.render_widget(Paragraph::new(Line::from(content)).style(style), name_area);
        frame.render_widget(
            Paragraph::new(counter)
                .style(style)
                .alignment(Alignment::Right),
            Rect::new(name_area.right(), rect.y, count_width, 1),
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::Playlist(*id),
            enabled: true,
        });
    }
}

fn ellipsize_name(name: &str, width: usize) -> String {
    let line = Line::raw(name);
    if line.width() <= width {
        return name.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut remaining = width - 1;
    let mut truncated = String::new();
    for grapheme in line.styled_graphemes(Style::default()) {
        let width = Span::raw(grapheme.symbol).width();
        if width > remaining {
            break;
        }
        truncated.push_str(grapheme.symbol);
        remaining -= width;
    }
    truncated.push('…');
    truncated
}

pub(super) fn tracks_table(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.view.tracks_area = area;
    let inner = area;
    if inner.height < 2 {
        return;
    }
    let filtering = app.view.filter_editing || !app.library.query.is_empty();
    let inner = if filtering {
        let right = 10u16.min(inner.width.saturating_sub(1));
        let field = Rect::new(inner.x, inner.y, inner.width.saturating_sub(right), 1);
        app.view.filter_field = field;
        let label = app.text("Filter: ", "Фильтр: ");
        let label_width = (Span::raw(label).width() as u16).min(field.width);
        frame.render_widget(
            Paragraph::new(label).style(palette.text().fg(palette.accent)),
            Rect::new(field.x, field.y, label_width, 1),
        );
        let input = Rect::new(
            field.x + label_width,
            field.y,
            field.width.saturating_sub(label_width),
            1,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{}{}",
                clean(&app.library.query),
                if app.view.filter_editing { "│" } else { "" }
            ))
            .scroll((
                0,
                Span::raw(&app.library.query)
                    .width()
                    .saturating_sub(usize::from(input.width.saturating_sub(1)))
                    as u16,
            ))
            .style(palette.text().bg(
                if app.view.filter_selected_all && app.view.filter_editing {
                    palette.selection
                } else {
                    palette.background
                },
            )),
            input,
        );
        app.view.hits.push(Hit {
            area: field,
            target: Target::FilterInput,
            enabled: true,
        });
        super::buttons::quiet_button(
            frame,
            app,
            Rect::new(inner.right() - right, inner.y, right.saturating_sub(3), 1),
            app.text("Keys", "Клав."),
            Target::QueryKeyboard,
            true,
            palette,
        );
        super::buttons::quiet_button(
            frame,
            app,
            Rect::new(inner.right().saturating_sub(3), inner.y, 3, 1),
            "x",
            Target::ClearFilter,
            true,
            palette,
        );
        Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1)
    } else {
        inner
    };
    let capacity = inner.height.saturating_sub(1) as usize;
    let entries = app.rows();
    let selected = entries
        .iter()
        .position(|entry| Some(entry.id) == app.library.selected_entry)
        .unwrap_or(0);
    let offset = visible_offset(app.view.track_offset, selected, capacity, entries.len());
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
            let selected = Some(entry.id) == app.library.selected_entry;
            let playing = app.playback_active()
                && app.playback.playing_playlist == app.library.selected_playlist
                && app.playback.playing_entry == Some(entry.id);
            let mark = if app.library.marked.contains(&entry.id) {
                "[x]"
            } else if selected {
                "[ ]"
            } else {
                "   "
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
                    format!("[{}]", if app.current_paused() { "‖" } else { "▶" }),
                    style
                        .bg(if app.current_paused() {
                            palette.muted
                        } else {
                            palette.accent
                        })
                        .fg(palette.background)
                        .add_modifier(Modifier::BOLD),
                ));
                title.push(Span::styled(" ", style));
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
    app.view.track_offset = offset;
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
        let active = sort == Some(app.library.sort);
        let mut style = palette.text().fg(if active {
            palette.accent
        } else {
            palette.muted
        });
        if sort.is_some() && app.hovered(rect) {
            style = style.bg(palette.selection).add_modifier(Modifier::BOLD);
        }
        let label = if active {
            format!(
                "{} {label}",
                if app.library.sort_descending {
                    "↓"
                } else {
                    "↑"
                }
            )
        } else {
            label.to_owned()
        };
        header_cells.push(Cell::from(label).style(style));
        if let Some(sort) = sort
            && rect.width > 0
        {
            app.view.hits.push(Hit {
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
            Paragraph::new(if !app.library.query.is_empty() {
                app.text(
                    "No matches. Clear the filter to show all tracks.",
                    "Ничего не найдено. Очистите фильтр для показа всех композиций.",
                )
            } else {
                app.text(
                    "Add music with Add ↓ or copy tracks from the sorting desk.",
                    "Добавьте музыку через «Добавить ↓» или скопируйте композиции из сортировочного стола.",
                )
            })
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
        app.view.hits.push(Hit {
            area: rect,
            target: Target::Track(id),
            enabled: true,
        });
        app.view.hits.push(Hit {
            area: Rect::new(rect.x, rect.y, 3.min(rect.width), 1),
            target: Target::Mark(id),
            enabled: true,
        });
    }
}

pub(super) fn filter_keyboard(frame: &mut Frame, app: &mut App, palette: Palette) {
    let field = app.view.filter_field;
    if field.is_empty() {
        return;
    }
    let area = frame.area();
    let width = 41u16.min(area.width);
    let height = 9u16.min(area.height.saturating_sub(3));
    let y = if field.bottom() + height <= area.bottom().saturating_sub(3) {
        field.bottom()
    } else {
        field.y.saturating_sub(height).max(area.y)
    };
    let rect = Rect::new(
        field.x.min(area.right().saturating_sub(width)),
        y,
        width,
        height,
    );
    app.view.filter_keyboard_area = rect;
    app.view.overlay_area = rect;
    app.view
        .hits
        .retain(|hit| hit.area.intersection(rect).is_empty());
    frame.render_widget(Clear, rect);
    let outer = block(app.text("Keyboard", "Клавиатура").into(), palette, true);
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    super::dialogs::keyboard(
        frame,
        app,
        inner,
        app.view.filter_keyboard_language,
        app.view.filter_keyboard_upper,
        palette,
    );
    button(
        frame,
        app,
        Rect::new(rect.right() - 4, rect.y, 3, 1),
        "x",
        Target::QueryKeyboard,
        true,
        palette,
    );
}
