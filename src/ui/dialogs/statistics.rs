use super::super::{
    theme::Palette,
    widgets::{block, buttons, clean, modal_rect, visible_offset},
};
use crate::{
    app::{App, Hit, StatisticsPage, Target},
    statistics::StatisticsRow,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Cell, Clear, Paragraph, Row, Table, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let rect = modal_rect(area, area.height);
    app.view.overlay_area = rect;
    frame.render_widget(Clear, rect);
    let outer = block(
        app.text("Listening statistics", "Статистика прослушивания")
            .into(),
        palette,
        app.statistics.view.focused,
    );
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    if inner.width < 8 || inner.height < 3 {
        return;
    }
    if inner.width < 24 || inner.height < 12 {
        frame.render_widget(
            Paragraph::new(format!(
                "{}: {}\n{}: {}\n{}",
                app.text("Time", "Время"),
                time(app.statistics.view.snapshot.summary.listened_ms),
                app.text("Plays", "Прослушано"),
                app.statistics.view.snapshot.summary.plays,
                app.text(
                    "Enlarge the window to see the lists.",
                    "Увеличьте окно для просмотра списков."
                )
            ))
            .style(palette.text())
            .wrap(Wrap { trim: true }),
            Rect::new(inner.x, inner.y, inner.width, inner.height - 1),
        );
        buttons(
            frame,
            app,
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            vec![(
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            )],
            palette,
        );
        return;
    }
    let compact = inner.width < 62;
    let summary = &app.statistics.view.snapshot.summary;
    let summary_lines = if compact {
        vec![
            format!(
                "{}: {}",
                app.text("Time", "Время"),
                time(summary.listened_ms)
            ),
            format!(
                "{}: {} · {}: {}",
                app.text("Plays", "Зачтено"),
                summary.plays,
                app.text("Starts", "Вкл."),
                summary.starts
            ),
            format!(
                "{} {} · {} {} · {} {}",
                app.text("Tr.", "Тр."),
                summary.tracks,
                app.text("Art.", "Исп."),
                summary.artists,
                app.text("Alb.", "Альб."),
                summary.albums
            ),
        ]
    } else {
        vec![
            format!(
                "{}: {}    {}: {}    {}: {}",
                app.text("Listening time", "Время прослушивания"),
                time(summary.listened_ms),
                app.text("Plays", "Прослушиваний"),
                summary.plays,
                app.text("Starts", "Включений"),
                summary.starts
            ),
            format!(
                "{}: {}    {}: {}    {}: {}",
                app.text("Tracks", "Треков"),
                summary.tracks,
                app.text("Artists", "Артистов"),
                summary.artists,
                app.text("Albums", "Альбомов"),
                summary.albums
            ),
        ]
    };
    let mut y = inner.y;
    for (index, line) in summary_lines.into_iter().enumerate() {
        frame.render_widget(
            Paragraph::new(line).style(if index == 0 {
                palette.text().fg(palette.accent)
            } else {
                palette.text()
            }),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
    }
    frame.render_widget(
        Paragraph::new(if compact {
            app.text(
                "Play: 50% or 4 min.\nPartials add time.",
                "Зачёт: 50% или 4 мин.\nОтрывки добавляют время.",
            )
        } else {
            app.text(
                "A play: 50% of the track or 4 min; partial listens add time.",
                "Прослушивание: 50% трека или 4 мин; отрывки добавляют время.",
            )
        })
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: true }),
        Rect::new(inner.x, y, inner.width, if compact { 2 } else { 1 }),
    );
    y += if compact { 2 } else { 1 };
    let tabs = StatisticsPage::ALL
        .into_iter()
        .map(|page| {
            (
                format!(
                    "{}{}",
                    if app.statistics.view.page == page {
                        "● "
                    } else {
                        ""
                    },
                    page.name(app.settings.preferences.language)
                ),
                Target::StatisticsPage(page),
                true,
            )
        })
        .collect();
    y += buttons(
        frame,
        app,
        Rect::new(inner.x, y, inner.width, 2),
        tabs,
        palette,
    );
    let recent = app.statistics.view.page == StatisticsPage::Recent;
    let mut controls = Vec::new();
    if !recent {
        controls.push((
            format!(
                "{}: {} ↓",
                app.text("Sort", "Сортировка"),
                app.statistics
                    .view
                    .sort
                    .name(app.settings.preferences.language)
            ),
            Target::StatisticsSort,
            true,
        ));
    }
    controls.push((
        app.text("Refresh", "Обновить").into(),
        Target::StatisticsRefresh,
        true,
    ));
    y += buttons(
        frame,
        app,
        Rect::new(inner.x, y, inner.width, 2),
        controls,
        palette,
    );
    let footer_y = inner.bottom().saturating_sub(1);
    buttons(
        frame,
        app,
        Rect::new(inner.x, footer_y, inner.width, 1),
        vec![(
            app.text("Close", "Закрыть").into(),
            Target::CloseDialog,
            true,
        )],
        palette,
    );
    if let Some(hit) = app
        .view
        .hits
        .iter()
        .rev()
        .find(|hit| matches!(hit.target, Target::CloseDialog))
    {
        let x = hit.area.right() + 1;
        let count = format!(
            "{}/{}",
            if app.statistics.view.rows.is_empty() {
                0
            } else {
                app.statistics.view.selected + 1
            },
            app.statistics.view.rows.len()
        );
        let available = inner.right().saturating_sub(x);
        let track_page = matches!(
            app.statistics.view.page,
            StatisticsPage::Tracks | StatisticsPage::Recent
        );
        let hint = if !app.statistics.view.focused {
            if available >= 32 {
                app.text(
                    "Player keys · click here to focus",
                    "Клавиши: плеер · клик: фокус",
                )
                .into()
            } else if available >= 12 {
                app.text("Click: focus", "Клик: фокус").into()
            } else {
                count
            }
        } else if track_page {
            let hint = format!(
                "{}   {count}",
                app.text(
                    "Enter: track · ←→ tabs · ↑↓ rows · S sort · R refresh",
                    "Enter: к треку · ←→ вкладки · ↑↓ строки · S сорт. · R обновить",
                )
            );
            let short = format!("{} · {count}", app.text("Enter: track", "Enter: трек"));
            if Span::raw(&hint).width() <= usize::from(available) {
                hint
            } else if Span::raw(&short).width() <= usize::from(available) {
                short
            } else {
                count
            }
        } else if compact {
            count
        } else {
            format!(
                "{}   {count}",
                app.text(
                    "←→ tabs · ↑↓ rows · S sort · R refresh",
                    "←→ вкладки · ↑↓ строки · S сорт. · R обновить"
                )
            )
        };
        frame.render_widget(
            Paragraph::new(hint).style(palette.text().fg(palette.muted)),
            Rect::new(x, footer_y, inner.right().saturating_sub(x), 1),
        );
    }
    let details_height = if inner.height >= 15 { 2 } else { 0 };
    let body = Rect::new(
        inner.x,
        y,
        inner.width,
        footer_y.saturating_sub(y + details_height),
    );
    if body.height == 0 {
        return;
    }
    let page = app.statistics.view.page;
    let row_height = if compact && page != StatisticsPage::Artists {
        2
    } else {
        1
    };
    let capacity = usize::from(body.height.saturating_sub(1) / row_height);
    app.statistics.view.capacity = capacity;
    app.statistics.view.offset = visible_offset(
        app.statistics.view.offset,
        app.statistics.view.selected,
        capacity,
        app.statistics.view.rows.len(),
    );
    let view = &app.statistics.view;
    let offset = view.offset;
    let rows = &view.rows;
    if let Some(error) = &view.error {
        frame.render_widget(
            Paragraph::new(format!(
                "{}: {}",
                app.text(
                    "Could not save/load history",
                    "Не удалось сохранить/загрузить историю"
                ),
                error
            ))
            .style(palette.text().fg(palette.error))
            .wrap(Wrap { trim: true }),
            body,
        );
    } else if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(if view.loading {
                app.text("Loading…", "Загрузка…")
            } else {
                app.text(
                    "Statistics start when you play music.",
                    "Статистика начнёт собираться, когда вы включите музыку.",
                )
            })
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true }),
            body,
        );
    } else {
        let show_artist = !compact && page != StatisticsPage::Artists;
        let show_date = recent && inner.width >= 80;
        let mut widths = vec![Constraint::Min(8)];
        let mut header = vec![Cell::from(page.name(app.settings.preferences.language))];
        if show_artist {
            widths.push(Constraint::Percentage(22));
            header.push(Cell::from(app.text("Artist", "Артист")));
        }
        if show_date {
            widths.push(Constraint::Length(16));
            header.push(Cell::from(app.text("Date", "Дата")));
        }
        if !recent{widths.push(Constraint::Length(if compact{4}else{6}));header.push(Cell::from(app.text("Starts","Вкл")));}
        widths.extend([
            Constraint::Length(if compact { 3 } else { 7 }),
            Constraint::Length(if compact { 8 } else { 10 }),
        ]);
        header.push(Cell::from(if recent {
            app.text("Play", "Зачёт")
        } else if compact {
            "#"
        } else {
            app.text("Plays", "Просл.")
        }));
        header.push(Cell::from(app.text("Time", "Время")));
        let table_rows = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(capacity)
            .map(|(index, row)| {
                let title = row_title(row, page, app);
                let mut lines = vec![Line::from(clean(title))];
                if row_height == 2 {
                    lines.push(Line::from(Span::styled(
                        clean(artist(row, app)),
                        palette.text().fg(palette.muted),
                    )));
                }
                let mut cells = vec![Cell::from(lines)];
                if show_artist {
                    cells.push(Cell::from(clean(artist(row, app))));
                }
                if show_date {
                    cells.push(Cell::from(row.date.clone()));
                }
                if !recent{cells.push(Cell::from(row.starts.to_string()));}
                cells.push(Cell::from(if recent {
                    if row.plays > 0 {
                        "✓".into()
                    } else {
                        "—".into()
                    }
                } else {
                    row.plays.to_string()
                }));
                cells.push(Cell::from(time(row.listened_ms)));
                Row::new(cells)
                    .height(row_height)
                    .style(if index == app.statistics.view.selected {
                        palette
                            .text()
                            .fg(palette.selection_text)
                            .bg(palette.selection)
                    } else {
                        palette.text()
                    })
            });
        frame.render_widget(
            Table::new(table_rows, widths).column_spacing(1).header(
                Row::new(header).style(
                    palette
                        .text()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ),
            body,
        );
        for index in offset..(offset + capacity).min(rows.len()) {
            app.view.hits.push(Hit {
                area: Rect::new(
                    body.x,
                    body.y + 1 + (index - offset) as u16 * row_height,
                    body.width,
                    row_height,
                ),
                target: Target::StatisticsRow(index),
                enabled: true,
            });
        }
    }
    if details_height > 0 {
        let detail = if let Some(row) = app.statistics.view.rows.get(app.statistics.view.selected) {
            if compact {
                match page {
                    StatisticsPage::Recent => format!(
                        "{} · {}\n{}",
                        row.date,
                        if row.plays > 0 {
                            "✓"
                        } else {
                            app.text("Partial", "Отрывок")
                        },
                        clean(if row.album.is_empty() {
                            app.text("No album", "Без альбома")
                        } else {
                            &row.album
                        })
                    ),
                    StatisticsPage::Tracks => format!(
                        "{}\n{}: {}",
                        clean(if row.album.is_empty() {
                            app.text("No album", "Без альбома")
                        } else {
                            &row.album
                        }),
                        app.text("Starts", "Включений"),
                        row.starts
                    ),
                    _ => format!(
                        "{}: {} · {}: {}\n{}",
                        app.text("Tr.", "Тр."),
                        row.tracks,
                        app.text("Starts", "Вкл."),
                        row.starts,
                        row.date
                    ),
                }
            } else {
                format!(
                    "{} · {} · {}: {} · {}: {}\n{} · {}: {}",
                    clean(artist(row, app)),
                    clean(if page == StatisticsPage::Albums {
                        row_title(row, page, app)
                    } else if row.album.is_empty() {
                        app.text("No album", "Без альбома")
                    } else {
                        &row.album
                    }),
                    app.text("Starts", "Включений"),
                    row.starts,
                    app.text("Tracks", "Треков"),
                    row.tracks,
                    row.date,
                    app.text("Heard", "Звучало"),
                    time(row.listened_ms)
                )
            }
        } else {
            app.text(
                "← →: tabs · ↑ ↓: rows · S: sort · R: refresh",
                "← →: вкладки · ↑ ↓: строки · S: сортировка · R: обновить",
            )
            .into()
        };
        frame.render_widget(
            Paragraph::new(detail).style(palette.text().fg(palette.muted)),
            Rect::new(
                inner.x,
                footer_y - details_height,
                inner.width,
                details_height,
            ),
        );
    }
}
fn artist<'a>(row: &'a StatisticsRow, app: &'a App) -> &'a str {
    if row.artist.is_empty() {
        app.text("Unknown artist", "Неизвестный артист")
    } else {
        &row.artist
    }
}
fn row_title<'a>(row: &'a StatisticsRow, page: StatisticsPage, app: &'a App) -> &'a str {
    if row.title.is_empty() {
        match page {
            StatisticsPage::Artists => app.text("Unknown artist", "Неизвестный артист"),
            StatisticsPage::Albums => app.text("No album", "Без альбома"),
            _ => app.text("Untitled", "Без названия"),
        }
    } else {
        &row.title
    }
}
fn time(ms: u64) -> String {
    let seconds = ms / 1000;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}
