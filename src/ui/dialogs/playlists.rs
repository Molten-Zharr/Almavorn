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
        playlist.display_name(app.settings.language)
    );
    let editable = playlist.can_edit(app.view.editing) && !app.busy();
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
        if app.settings.scan_subfolders {
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
                "Add a folder here or import one from Add > Add folder.",
                "Добавьте папку здесь или через Добавить > Добавить папку.",
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
    area: Rect,
    palette: Palette,
) {
    let choices: Vec<_> = app
        .compose_choices()
        .into_iter()
        .map(|item| {
            (
                item.id,
                item.display_name(app.settings.language).to_owned(),
                item.entries.len(),
            )
        })
        .collect();
    *selected = (*selected).min(choices.len().saturating_sub(1));
    let selected_order = choices
        .get(*selected)
        .and_then(|(id, _, _)| ids.iter().position(|value| value == id));
    let inner = modal(
        frame,
        app,
        area,
        app.text("Compose playlists", "Собрать плейлист").into(),
        30,
        palette,
    );
    let actions = vec![
        (
            app.text("Earlier", "Раньше").into(),
            Target::ComposeMove(-1),
            selected_order.is_some_and(|index| index > 0),
        ),
        (
            app.text("Later", "Позже").into(),
            Target::ComposeMove(1),
            selected_order.is_some_and(|index| index + 1 < ids.len()),
        ),
        (
            app.text("Create", "Создать").into(),
            Target::Submit,
            ids.len() >= 2 && !app.busy(),
        ),
        (
            app.text("Cancel", "Отмена").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    let footer = action_rows(app, inner.width, &actions).min(inner.height);
    let hint = app.text("Space or click: select. Numbers show the chosen order; Ctrl+Up/Down changes it. Enter: name the new playlist. Tracks keep their order, repeats are skipped, original playlists are kept.", "Space или клик: отметить. Номера показывают порядок сборки; Ctrl+↑↓ меняет его. Enter: имя нового плейлиста. Порядок композиций сохраняется, повторы пропускаются, исходные плейлисты остаются.");
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
            let count = format!(" · {count}");
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
