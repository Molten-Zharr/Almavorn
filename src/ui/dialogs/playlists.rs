use super::super::{
    theme::Palette,
    widgets::{button, button_width, buttons, clean, modal, visible_offset},
};
use crate::app::{App, Hit, Target};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn folders(
    frame: &mut Frame,
    app: &mut App,
    id: i64,
    selected: &mut usize,
    area: Rect,
    palette: Palette,
) {
    let Some(playlist) = app.playlists().iter().find(|playlist| playlist.id == id) else {
        return;
    };
    let title = format!(
        "{} · {}",
        app.text("Playlist folders", "Папки плейлиста"),
        playlist.display_name(app.settings.preferences.language)
    );
    let editable = app.playlist_editable(playlist) && !app.busy();
    let paths: Vec<_> = playlist
        .folders
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let scan = !app.busy() && app.allowed(crate::input::Action::AddNew);
    let inner = modal(frame, app, area, clean(&title), 24, palette);
    let actions = vec![
        (
            app.text("Link folder", "Добавить папку").into(),
            Target::FolderAdd,
            editable,
        ),
        (
            app.text("Change path", "Изменить путь").into(),
            Target::FolderEdit,
            editable && !paths.is_empty(),
        ),
        (
            app.text("Unlink", "Отключить").into(),
            Target::FolderRemove,
            editable && !paths.is_empty(),
        ),
        (
            app.text("Find new", "Найти новые").into(),
            Target::FolderScan,
            scan,
        ),
        (
            app.text("Close", "Закрыть").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    let footer_rows = action_rows(app, inner.width, &actions).min(inner.height);
    let hint = app.text("Find new music in linked folders. The checkbox includes subfolders; otherwise only files directly in each folder are checked. Unlinking keeps tracks.", "Поиск новых треков идет в связанных папках. Галочка включает подпапки; без нее проверяются только файлы в каждой папке. Отключение сохраняет композиции.");
    let hints = super::help::wrapped_height(hint, inner.width)
        .min(inner.height.saturating_sub(footer_rows + 6));
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, hints),
    );
    let checkbox_y = inner.y + hints + u16::from(hints > 0);
    let checkbox = format!(
        "[{}] {}",
        if app.settings.preferences.scan_subfolders {
            'x'
        } else {
            ' '
        },
        app.text("Search subfolders", "Искать в подпапках")
    );
    button(
        frame,
        app,
        Rect::new(
            inner.x,
            checkbox_y,
            button_width(app, &checkbox, &Target::ScanSubfolders).min(inner.width),
            1,
        ),
        &checkbox,
        Target::ScanSubfolders,
        !app.busy(),
        palette,
    );
    let list = Rect::new(
        inner.x,
        checkbox_y + 2,
        inner.width,
        inner
            .bottom()
            .saturating_sub(footer_rows + 1)
            .saturating_sub(checkbox_y + 2),
    );
    let capacity = usize::from(list.height / 2);
    *selected = (*selected).min(paths.len().saturating_sub(1));
    let offset = visible_offset(0, *selected, capacity, paths.len());
    if paths.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text(
                "Link a folder here, or create a playlist using Folders to playlists.",
                "Добавьте связанную папку здесь или создайте плейлист через Папки в плейлисты.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            list,
        );
    }
    for (row, (index, path)) in paths
        .iter()
        .enumerate()
        .skip(offset)
        .take(capacity)
        .enumerate()
    {
        let rect = Rect::new(list.x, list.y + row as u16 * 2, list.width, 2);
        let style = if index == *selected {
            palette.text().bg(palette.selection)
        } else if app.hovered(rect) {
            palette.text().fg(palette.accent)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(clean(path))
                .style(style)
                .wrap(Wrap { trim: false }),
            rect,
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::FolderRow(index),
            enabled: true,
        });
    }
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer_rows),
            inner.width,
            footer_rows,
        ),
        actions,
        palette,
    );
}

fn action_rows(app: &App, width: u16, actions: &[(String, Target, bool)]) -> u16 {
    let mut x = 0u16;
    let mut rows = 1;
    for (label, target, _) in actions {
        let size = button_width(app, label, target).min(width);
        if x > 0 && x.saturating_add(size) > width {
            x = 0;
            rows += 1;
        }
        x = x.saturating_add(size + 1);
    }
    rows
}

pub(super) fn compose(
    frame: &mut Frame,
    app: &mut App,
    selected: &mut usize,
    ids: &[i64],
    entries: Option<&[i64]>,
    area: Rect,
    palette: Palette,
) {
    let tracks = entries.is_some();
    let choices: Vec<_> = if let Some(entries) = entries {
        let by_id: std::collections::HashMap<_, _> = app
            .playlists()
            .iter()
            .flat_map(|playlist| {
                playlist
                    .entries
                    .iter()
                    .map(move |entry| (entry.id, (playlist, entry)))
            })
            .collect();
        entries
            .iter()
            .map(|id| {
                if let Some((playlist, entry)) = by_id.get(id) {
                    (
                        *id,
                        format!(
                            "{}{} · {}",
                            entry.track.title,
                            if entry.track.artist.is_empty() {
                                String::new()
                            } else {
                                format!(" — {}", entry.track.artist)
                            },
                            playlist.display_name(app.settings.preferences.language)
                        ),
                        0,
                    )
                } else {
                    (
                        *id,
                        app.text(
                            "Track no longer available · return to sources",
                            "Композиция недоступна · вернитесь к источникам",
                        )
                        .to_owned(),
                        0,
                    )
                }
            })
            .collect()
    } else {
        app.compose_choices()
            .into_iter()
            .map(|playlist| {
                (
                    playlist.id,
                    if playlist.kind == crate::model::PlaylistKind::SortingDesk {
                        playlist
                            .display_name(app.settings.preferences.language)
                            .to_owned()
                    } else {
                        format!(
                            "{} · {}",
                            playlist.display_name(app.settings.preferences.language),
                            playlist.mode.name(app.settings.preferences.language)
                        )
                    },
                    playlist.entries.len(),
                )
            })
            .collect()
    };
    *selected = (*selected).min(choices.len().saturating_sub(1));
    let selected_order = if tracks {
        (!choices.is_empty()).then_some(*selected)
    } else {
        choices
            .get(*selected)
            .and_then(|(id, _, _)| ids.iter().position(|value| value == id))
    };
    let ordered_len = if tracks { choices.len() } else { ids.len() };
    let inner = modal(
        frame,
        app,
        area,
        if tracks {
            app.text(
                "Build mix · 2/2 · Tracks and order",
                "Собрать подборку · 2/2 · Композиции и порядок",
            )
        } else {
            app.text(
                "Build mix · 1/2 · Sources",
                "Собрать подборку · 1/2 · Источники",
            )
        }
        .into(),
        30,
        palette,
    );
    let mut actions = vec![
        (
            app.text("Earlier", "Раньше").into(),
            Target::ComposeMove(-1),
            selected_order.is_some_and(|index| index > 0),
        ),
        (
            app.text("Later", "Позже").into(),
            Target::ComposeMove(1),
            selected_order.is_some_and(|index| index + 1 < ordered_len),
        ),
        (
            if tracks {
                app.text("Save mix…", "Сохранить подборку…")
            } else {
                app.text("Choose tracks", "Выбрать композиции")
            }
            .into(),
            Target::Submit,
            !ids.is_empty() && !app.busy(),
        ),
        (
            app.text("Cancel", "Отмена").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    if tracks {
        actions.insert(
            0,
            (
                app.text("Sources", "К источникам").into(),
                Target::ComposeBack,
                true,
            ),
        );
        actions.insert(
            1,
            (
                app.text("Select all", "Отметить все").into(),
                Target::ComposeAll(true),
                true,
            ),
        );
        actions.insert(
            2,
            (
                app.text("Clear selection", "Снять отметки").into(),
                Target::ComposeAll(false),
                true,
            ),
        );
    }
    let footer = action_rows(app, inner.width, &actions).min(inner.height);
    let hint = if tracks {
        app.text("Space/click: select tracks. Ctrl+Up/Down: move a row. Ctrl+A: all; Ctrl+Shift+A: none. A new mix will be created. Source playlists are kept.", "Space/клик: отметить композиции. Ctrl+↑↓: переместить строку. Ctrl+A: все; Ctrl+Shift+A: снять отметки. Будет создана новая подборка. Исходные плейлисты сохранятся.")
    } else {
        app.text("Select source playlists, including protected Order playlists. Ctrl+Up/Down: source order. Next, choose tracks and arrange them. Repeated tracks are included once.", "Отметьте источники, в том числе защищенные плейлисты Порядка. Ctrl+↑↓: порядок источников. Далее выберите композиции и их последовательность. Повторные композиции попадут в подборку один раз.")
    };
    let hints =
        super::help::wrapped_height(hint, inner.width).min(inner.height.saturating_sub(footer + 3));
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, hints),
    );
    let list = Rect::new(
        inner.x,
        inner.y + hints + 1,
        inner.width,
        inner.height.saturating_sub(hints + footer + 2),
    );
    if choices.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text(
                "No tracks in these sources. Go back and choose other playlists.",
                "В этих источниках нет композиций. Вернитесь и выберите другие плейлисты.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            list,
        );
    }
    let rows: Vec<_> = choices
        .iter()
        .map(|(id, name, count)| {
            let rank = ids.iter().position(|value| value == id);
            let label = format!(
                "[{}] {} {}",
                if rank.is_some() { 'x' } else { ' ' },
                rank.map(|index| format!("{}.", index + 1))
                    .unwrap_or_else(|| "  ".into()),
                clean(name)
            );
            let count = if tracks {
                String::new()
            } else {
                format!(" · {count}")
            };
            let count_width = (ratatui::text::Span::raw(&count).width() as u16).min(list.width);
            let height =
                super::help::wrapped_height(&label, list.width.saturating_sub(count_width))
                    .min(list.height)
                    .max(1);
            (label, count, count_width, height)
        })
        .collect();
    let mut offset = 0;
    let mut used: usize = rows
        .iter()
        .take(*selected + 1)
        .map(|row| usize::from(row.3))
        .sum();
    while used > usize::from(list.height) && offset < *selected {
        used -= usize::from(rows[offset].3);
        offset += 1;
    }
    let mut y = list.y;
    for (index, (label, count, count_width, height)) in rows.iter().enumerate().skip(offset) {
        if y.saturating_add(*height) > list.bottom() {
            break;
        }
        let rect = Rect::new(list.x, y, list.width, *height);
        y += height;
        let style = if index == *selected {
            palette.text().bg(palette.selection)
        } else if app.hovered(rect) {
            palette.text().fg(palette.accent)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(label.as_str())
                .style(style)
                .wrap(Wrap { trim: false }),
            Rect::new(
                rect.x,
                rect.y,
                rect.width.saturating_sub(*count_width),
                *height,
            ),
        );
        frame.render_widget(
            Paragraph::new(count.as_str()).style(style),
            Rect::new(
                rect.right().saturating_sub(*count_width),
                rect.y,
                *count_width,
                *height,
            ),
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::ComposeRow(index),
            enabled: true,
        });
    }
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer),
            inner.width,
            footer,
        ),
        actions,
        palette,
    );
}
