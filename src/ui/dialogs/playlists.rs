use super::super::{
    theme::Palette,
    widgets::{button_width, buttons, clean, modal, visible_offset},
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
    let hint = app.text("Linked folders and subfolders are scanned for new tracks. Unlinking keeps music in the library. Order changes require editing.", "Поиск новых треков идет в связанных и вложенных папках. Отключение папки сохраняет композиции в библиотеке. Правки в Порядке требуют редактирования.");
    let hints = super::help::wrapped_height(hint, inner.width)
        .min(inner.height.saturating_sub(footer_rows + 4));
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, hints),
    );
    let list = Rect::new(
        inner.x,
        inner.y + hints + u16::from(hints > 0),
        inner.width,
        inner
            .height
            .saturating_sub(hints + u16::from(hints > 0) + footer_rows + 1),
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
