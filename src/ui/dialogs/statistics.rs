use super::super::{
    theme::Palette,
    widgets::{block, buttons, button_width, clean, modal_rect, visible_offset},
};
use crate::{
    app::{App, Hit, StatisticsControl, StatisticsInfo, StatisticsMenu, StatisticsPage, StatisticsSort, Target},
    statistics::StatisticsRow,
};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Cell, Clear, Paragraph, Row, Table, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let rect = modal_rect(area, area.height);
    app.view.overlay_background = (app.view.hits.len(), app.view.keycaps.len());
    app.view.overlay_area = rect;
    frame.render_widget(Clear, rect);
    let outer = block(
        app.text("Listening statistics", "Статистика прослушивания")
            .into(),
        palette,
        app.statistics.view.focused,
    );
    let inner = outer.inner(rect).inner(Margin::new(1, 1));
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
    frame.render_widget(
        Paragraph::new(format!("{}: {}", app.text("Total listening time", "Всего прослушано"),
            time(app.statistics.view.snapshot.summary.listened_ms))).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let recent = app.statistics.view.page == StatisticsPage::Recent;
    let mut entries = vec![(
        format!("{} ↓", app.statistics.view.page.name(app.settings.preferences.language)),
        Target::StatisticsControl(StatisticsControl::Menu(StatisticsMenu::Page)), true,
    )];
    entries.push((format!("{}: {} {}", app.text("Sort", "Сортировка"), app.statistics.view.sort.name(app.settings.preferences.language), app.statistics.view.sort_arrow()),
        Target::StatisticsControl(StatisticsControl::Menu(StatisticsMenu::Sort)), true));
    entries.push((app.text("More ↓", "Еще ↓").into(), Target::StatisticsControl(StatisticsControl::Menu(StatisticsMenu::More)), true));
    let y = inner.y + 2;
    let used = controls(frame, app, Rect::new(inner.x, y, inner.width, inner.height.saturating_sub(6)), entries, palette);
    let y = y + used + 1;
    let footer_y = inner.bottom().saturating_sub(2);
    let mut footer = vec![(app.text("Close", "Закрыть").into(), Target::CloseDialog, true)];
    if inner.width >= 36 {
        footer.push((app.text("Go to track", "К композиции").into(), Target::StatisticsControl(StatisticsControl::OpenTrack),
            app.statistics.view.rows.get(app.statistics.view.selected).is_some_and(|row| row.track_id.is_some()) && !app.busy()));
        footer.push((app.text("Details", "Сведения").into(), Target::StatisticsControl(StatisticsControl::Info(StatisticsInfo::Selection)), !app.statistics.view.rows.is_empty()));
    }
    controls(frame, app, Rect::new(inner.x, footer_y, inner.width, 2), footer, palette);
    let body = Rect::new(inner.x, y, inner.width, footer_y.saturating_sub(y + 1));
    if body.height == 0 { return; }
    let page = app.statistics.view.page;
    let row_height = if body.height >= 12 { 2 } else { 1 };
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
        let mut headings = vec![(page.name(app.settings.preferences.language), StatisticsSort::Name)];
        if show_artist {
            widths.push(Constraint::Percentage(22));
            headings.push((app.text("Artist", "Артист"), StatisticsSort::Artist));
        }
        if show_date {
            widths.push(Constraint::Length(16));
            headings.push((app.text("Date", "Дата"), StatisticsSort::Date));
        }
        if !recent{widths.push(Constraint::Length(if compact{4}else{6}));headings.push((app.text("Starts","Вкл"), StatisticsSort::Starts));}
        widths.extend([
            Constraint::Length(if compact { 3 } else { 7 }),
            Constraint::Length(if compact { 8 } else { 10 }),
        ]);
        headings.push((if compact {
            "#"
        } else if recent {
            app.text("Play", "Зачет")
        } else {
            app.text("Plays", "Просл.")
        }, StatisticsSort::Plays));
        headings.push((app.text("Time", "Время"), StatisticsSort::Time));
        let columns = Layout::horizontal(widths.clone()).flex(Flex::Start).spacing(1)
            .split(Rect::new(body.x, body.y, body.width, 1));
        let header: Vec<_> = headings.iter().zip(columns.iter()).map(|((label, sort), rect)| {
            let active = *sort == app.statistics.view.sort;
            let style = if app.hovered(*rect) { palette.text().fg(palette.selection_text).bg(palette.selection).add_modifier(Modifier::UNDERLINED) }
                else { palette.text().fg(palette.accent) };
            Cell::from(format!("{}{label}", if active { app.statistics.view.sort_arrow() } else { "" })).style(style.add_modifier(Modifier::BOLD))
        }).collect();
        let table_rows = rows
            .iter()
            .enumerate()
            .skip(offset)
            .take(capacity)
            .map(|(index, row)| {
                let title = row_title(row, page, app);
                let mut lines = vec![Line::from(clean(title))];
                if compact && row_height == 2 && page != StatisticsPage::Artists {
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
            Table::new(table_rows, widths).flex(Flex::Start).column_spacing(1).header(
                Row::new(header).style(
                    palette
                        .text()
                        .fg(palette.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            ),
            body,
        );
        for ((_, sort), rect) in headings.iter().zip(columns.iter()) {
            app.view.hits.push(Hit { area: *rect, target: Target::StatisticsControl(StatisticsControl::Sort(*sort)), enabled: true });
        }
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
    render_popup(frame, app, inner, palette);
}

fn controls(frame: &mut Frame, app: &mut App, area: Rect, entries: Vec<(String, Target, bool)>, palette: Palette) -> u16 {
    let mut x = area.x;
    let mut y = area.y;
    for (label, target, enabled) in entries {
        let width = button_width(app, &label, &target).saturating_add(4).min(area.width);
        if x > area.x && x.saturating_add(width) > area.right() { x = area.x; y += 2; }
        if y + 2 > area.bottom() { break; }
        super::super::buttons::quiet_button(frame, app, Rect::new(x, y, width, 2), &label, target, enabled, palette);
        x = x.saturating_add(width + 2);
    }
    y.saturating_sub(area.y) + 2
}

fn render_popup(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if let Some(menu) = app.statistics.view.menu {
        let items: Vec<_> = app.statistics_menu_items(menu).into_iter().enumerate().map(|(index, item)| crate::app::CommandItem {
            label: item.label, target: Target::StatisticsControl(StatisticsControl::Item(index)), enabled: item.enabled,
        }).collect();
        let width = items.iter().map(|i| Span::raw(&i.label).width() as u16 + 6).max().unwrap_or(20).min(area.width);
        let height = super::super::dropdown::height(app, &items, false).min(area.height);
        let anchor = app.statistics.view.menu_anchor;
        let rect = Rect::new(anchor.x.clamp(area.x, area.right().saturating_sub(width)),
            anchor.y.clamp(area.y, area.bottom().saturating_sub(height)), width, height);
        app.statistics.view.popup_area = rect;
        app.view.hits.push(Hit { area: frame.area(), target: Target::StatisticsControl(StatisticsControl::Dismiss), enabled: true });
        super::super::dropdown::render(frame, app, rect, String::new(), &items, app.statistics.view.menu_selected, None, palette, |_, item| item.target.clone());
    } else if let Some(info) = app.statistics.view.info {
        let (title, text) = information(app, info);
        let width = area.width.min(72);
        let text_width = width.saturating_sub(4).max(1);
        let paragraph = Paragraph::new(text).style(palette.text()).wrap(Wrap { trim: false });
        let lines = paragraph.line_count(text_width);
        let height = (lines as u16).saturating_add(6).min(area.height);
        let rect = Rect::new(area.x + (area.width - width) / 2, area.y + (area.height - height) / 2, width, height);
        app.statistics.view.popup_area = rect;
        app.view.hits.push(Hit { area: frame.area(), target: Target::StatisticsControl(StatisticsControl::Dismiss), enabled: true });
        frame.render_widget(Clear, rect);
        let outer = block(title.into(), palette, true);
        let inner = outer.inner(rect).inner(Margin::new(1, 1));
        frame.render_widget(outer, rect);
        let body = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2));
        app.statistics.view.info_offset = app.statistics.view.info_offset.min(lines.saturating_sub(usize::from(body.height)));
        frame.render_widget(paragraph.scroll((app.statistics.view.info_offset.min(u16::MAX as usize) as u16, 0)), body);
        app.view.hits.push(Hit { area: body, target: Target::StatisticsControl(StatisticsControl::Info(info)), enabled: true });
        controls(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2), vec![
            (app.text("Back", "Назад").into(), Target::StatisticsControl(StatisticsControl::Dismiss), true),
        ], palette);
    } else { app.statistics.view.popup_area = Rect::default(); }
}

fn information(app: &App, info: StatisticsInfo) -> (&'static str, String) {
    match info {
        StatisticsInfo::Totals => {
            let s = &app.statistics.view.snapshot.summary;
            (app.text("Overall totals", "Общие итоги"), format!("{}: {}\n{}: {}\n{}: {}\n\n{}: {}\n{}: {}\n{}: {}",
                app.text("Listening time", "Время прослушивания"), time(s.listened_ms),
                app.text("Plays", "Прослушивания"), s.plays, app.text("Starts", "Включения"), s.starts,
                app.text("Tracks", "Композиции"), s.tracks, app.text("Artists", "Исполнители"), s.artists, app.text("Albums", "Альбомы"), s.albums))
        }
        StatisticsInfo::Counting => (app.text("How plays are counted", "Как считаются прослушивания"), app.text(
            "Start: each playback start.\n\nPlay: at least half of the track or four minutes. Shorter listens add to the listening time.\n\nSelect a track and choose Go to track, or double-click it. S opens sorting, M opens categories, R refreshes the list. Esc closes this panel first.",
            "Включение: каждый запуск композиции.\n\nПрослушивание: не меньше половины трека или четырех минут. Короткие отрывки добавляются ко времени прослушивания.\n\nВыберите строку и нажмите К композиции или дважды щелкните по ней. S открывает сортировку, M - категории, R обновляет список. Esc сначала закрывает эту панель."
        ).into()),
        StatisticsInfo::Selection => {
            let title = app.text("Selection details", "Сведения о выбранном");
            let Some(row) = app.statistics.view.rows.get(app.statistics.view.selected) else { return (title, String::new()); };
            (title, format!("{}\n{}\n{}\n\n{}: {}\n{}: {}\n{}: {}\n{}: {}\n{}: {}",
                clean(row_title(row, app.statistics.view.page, app)), clean(artist(row, app)), clean(&row.album),
                app.text("Starts", "Включения"), row.starts, app.text("Plays", "Прослушивания"), row.plays,
                app.text("Listening time", "Время прослушивания"), time(row.listened_ms),
                app.text("Tracks", "Композиции"), row.tracks, app.text("Last played", "Последнее включение"), row.date))
        }
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
