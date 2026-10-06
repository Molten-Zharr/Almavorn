use super::super::{
    theme::Palette,
    widgets::{button, button_width, buttons, clean, modal, modal_rect, visible_offset},
};
use crate::{
    app::{App, Hit, SearchDialog, SearchFocus, Target},
    model::duration_text,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    search: &mut SearchDialog,
    area: Rect,
    palette: Palette,
) {
    app.view.search_area = modal_rect(area, 46);
    let inner = modal(
        frame,
        area,
        app.text("Search · selected playlists", "Поиск · выбранные плейлисты")
            .into(),
        46,
        palette,
    );
    if inner.height < 10 {
        return;
    }
    frame.render_widget(
        Paragraph::new(if search.focus == SearchFocus::Playlists {
            app.text(
                "Space / Enter: select · Tab: section",
                "Space / Enter: выбрать · Tab: раздел",
            )
        } else {
            app.text(
                "Type to search · Tab: section · Enter: play",
                "Ввод: поиск · Tab: раздел · Enter: играть",
            )
        })
        .style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let field = Rect::new(inner.x, inner.y + 1, inner.width, 3);
    let active = search.focus == SearchFocus::Input;
    frame.render_widget(
        Paragraph::new(format!(
            "{}{}",
            clean(&search.query),
            if active { "│" } else { "" }
        ))
        .scroll((
            0,
            Line::from(search.query.as_str())
                .width()
                .saturating_sub(usize::from(field.width.saturating_sub(4))) as u16,
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.text().fg(if active {
                    palette.accent
                } else {
                    palette.inactive_panel_border
                })),
        )
        .style(palette.text().bg(if search.selected_all {
            palette.selection
        } else {
            palette.background
        })),
        field,
    );
    app.view.hits.push(Hit {
        area: field,
        target: Target::SearchInput,
        enabled: true,
    });
    let keyboard_height = if search.keyboard_visible && inner.height >= 19 {
        7
    } else {
        0
    };
    if keyboard_height > 0 {
        super::keyboard(
            frame,
            app,
            Rect::new(inner.x, field.bottom(), inner.width, keyboard_height),
            search.keyboard,
            search.upper,
            palette,
        );
    }
    let count_y = field.bottom() + keyboard_height;
    let keys = app.text("Keyboard", "Клавиатура");
    let keys_width = button_width(app, keys, &Target::QueryKeyboard).min(inner.width);
    button(
        frame,
        app,
        Rect::new(inner.right() - keys_width, count_y, keys_width, 1),
        keys,
        Target::QueryKeyboard,
        inner.height >= 19,
        palette,
    );
    frame.render_widget(
        Paragraph::new(if search.loading {
            app.text("Searching…", "Поиск…").into()
        } else {
            format!("{}: {}", app.text("Found", "Найдено"), search.results.len())
        })
        .style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, count_y, inner.width.saturating_sub(keys_width), 1),
    );
    let scope_rows = (app.playlists().len() as u16)
        .clamp(1, 4)
        .min(inner.height.saturating_sub(8 + keyboard_height));
    let scope_y = inner.bottom().saturating_sub(3 + scope_rows);
    let result_area = Rect::new(
        inner.x,
        count_y + 1,
        inner.width,
        scope_y.saturating_sub(count_y + 2),
    );
    search.results_area = result_area;
    let narrow = inner.width < 60;
    let row_height = if narrow { 2 } else { 1 };
    let capacity = usize::from(result_area.height.saturating_sub(1) / row_height);
    search.offset = visible_offset(
        search.offset,
        search.selected,
        capacity,
        search.results.len(),
    );
    let mut rows = Vec::new();
    let mut hits = Vec::new();
    for (index, hit) in search
        .results
        .iter()
        .enumerate()
        .skip(search.offset)
        .take(capacity)
    {
        let Some((playlist, entry)) = app.search_entry(*hit) else {
            continue;
        };
        let selected = index == search.selected;
        let playing = app.playback_active()
            && app.playback.playing_playlist == Some(playlist.id)
            && app.playback.playing_entry == Some(entry.id);
        let name = clean(playlist.display_name(app.settings.language));
        let prefix = if playing {
            format!(
                "[{}] ",
                if app.current_paused() {
                    app.text("PAUSED", "ПАУЗА")
                } else {
                    app.text("PLAYING", "ИГРАЕТ")
                }
            )
        } else {
            String::new()
        };
        let title = if narrow {
            format!("{prefix}{}\n{name}", clean(&entry.track.title))
        } else {
            format!("{prefix}{}", clean(&entry.track.title))
        };
        let hit_area = Rect::new(
            result_area.x,
            result_area.y + 1 + (index - search.offset) as u16 * row_height,
            result_area.width,
            row_height,
        );
        let style = if selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else if app.hovered(hit_area) {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        rows.push(
            Row::new(vec![
                Cell::from(title),
                Cell::from(if narrow {
                    String::new()
                } else {
                    clean(&entry.track.artist)
                }),
                Cell::from(if narrow { String::new() } else { name }),
                Cell::from(duration_text(entry.track.duration_ms)),
            ])
            .height(row_height)
            .style(style),
        );
        hits.push(Hit {
            area: Rect::new(
                result_area.x,
                result_area.y + 1 + (index - search.offset) as u16 * row_height,
                result_area.width,
                row_height,
            ),
            target: Target::SearchResult(index),
            enabled: true,
        });
    }
    let widths = if narrow {
        vec![
            Constraint::Min(10),
            Constraint::Length(0),
            Constraint::Length(0),
            Constraint::Length(6),
        ]
    } else {
        vec![
            Constraint::Percentage(42),
            Constraint::Percentage(22),
            Constraint::Min(10),
            Constraint::Length(6),
        ]
    };
    frame.render_widget(
        Table::new(rows, widths).column_spacing(1).header(
            Row::new(vec![
                app.text("Title", "Название"),
                if narrow {
                    ""
                } else {
                    app.text("Artist", "Исполнитель")
                },
                if narrow {
                    ""
                } else {
                    app.text("Playlist", "Плейлист")
                },
                app.text("Time", "Время"),
            ])
            .style(palette.text().fg(palette.muted)),
        ),
        result_area,
    );
    if search.results.is_empty() && !search.loading {
        frame.render_widget(
            Paragraph::new(if search.failed {
                app.text("Search interrupted", "Поиск прерван")
            } else if search.playlists.is_empty() {
                app.text("Choose playlists below", "Выберите плейлисты снизу")
            } else {
                app.text("No matches", "Ничего не найдено")
            })
            .style(palette.text().fg(palette.muted)),
            Rect::new(
                result_area.x,
                result_area.y + 1,
                result_area.width,
                result_area.height.saturating_sub(1),
            ),
        );
    }
    app.view.hits.extend(hits);
    let scope_label = app.text("Playlists", "Плейлисты");
    let label_width = Line::from(scope_label).width() as u16;
    frame.render_widget(
        Paragraph::new(scope_label).style(
            palette
                .text()
                .fg(if search.focus == SearchFocus::Playlists {
                    palette.accent
                } else {
                    palette.muted
                })
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(inner.x, scope_y, label_width, 1),
    );
    buttons(
        frame,
        app,
        Rect::new(
            inner.x + label_width + 1,
            scope_y,
            inner.width.saturating_sub(label_width + 1),
            1,
        ),
        vec![
            (app.text("All", "Все").into(), Target::SearchAll(true), true),
            (
                app.text("None", "Нет").into(),
                Target::SearchAll(false),
                true,
            ),
        ],
        palette,
    );
    search.playlists_area = Rect::new(inner.x, scope_y + 1, inner.width, scope_rows);
    search.playlist_offset = visible_offset(
        search.playlist_offset,
        search.playlist_selected,
        usize::from(scope_rows),
        app.playlists().len(),
    );
    let scope_items: Vec<_> = app
        .playlists()
        .iter()
        .enumerate()
        .skip(search.playlist_offset)
        .take(usize::from(scope_rows))
        .map(|(i, p)| {
            (
                i,
                p.id,
                format!(
                    "[{}] {} · {}",
                    if search.playlists.contains(&p.id) {
                        "x"
                    } else {
                        " "
                    },
                    clean(p.display_name(app.settings.language)),
                    p.mode.name(app.settings.language)
                ),
            )
        })
        .collect();
    for (row, (index, id, label)) in scope_items.into_iter().enumerate() {
        let rect = Rect::new(inner.x, scope_y + 1 + row as u16, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::raw(label)])).style(
                if (search.focus == SearchFocus::Playlists && index == search.playlist_selected)
                    || app.hovered(rect)
                {
                    palette.text().bg(palette.selection)
                } else {
                    palette.text()
                },
            ),
            rect,
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::SearchPlaylist(id),
            enabled: true,
        });
    }
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 2),
        vec![
            (
                app.text("Play", "Играть").into(),
                Target::SearchPlay,
                !search.results.is_empty(),
            ),
            (
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}
