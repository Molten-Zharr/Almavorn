use super::super::{
    theme::Palette,
    widgets::{buttons, buttons_height, clean, modal, visible_offset},
};
use crate::{
    app::{App, Hit, MoveFiles, Target},
    model::PlaylistKind,
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    selected: usize,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Tracks to playlist", "Композиции в плейлист")
            .into(),
        26,
        palette,
    );
    let entries: Vec<_> = app
        .transfer_destinations()
        .into_iter()
        .map(|p| {
            (
                format!(
                    "{} · {}{}",
                    clean(p.display_name(app.settings.preferences.language)),
                    p.mode.name(app.settings.preferences.language),
                    if app.playlist_editable(p) {
                        ""
                    } else {
                        app.text(" · Protected", " · Защищен")
                    }
                ),
                app.playlist_editable(p),
                p.kind,
            )
        })
        .collect();
    let editable = entries.get(selected).is_some_and(|p| p.1);
    let mut actions = vec![(
        app.text(
            if editable { "Copy" } else { "Edit playlist" },
            if editable {
                "Копировать"
            } else {
                "Редактировать"
            },
        )
        .into(),
        if editable {
            Target::Submit
        } else {
            Target::TransferEdit
        },
        !entries.is_empty(),
    )];
    if app
        .playlist()
        .is_some_and(|p| p.kind == PlaylistKind::SortingDesk)
    {
        actions.push((
            app.text("Move files", "Перенести файлы").into(),
            Target::TransferMove,
            editable
                && entries
                    .get(selected)
                    .is_some_and(|p| p.2 == PlaylistKind::Normal),
        ));
    }
    actions.push((
        app.text("Cancel", "Отмена").into(),
        Target::CloseDialog,
        true,
    ));
    let footer = buttons_height(app, inner.width, &actions).min(inner.height);
    let hint_height = 3.min(inner.height.saturating_sub(footer));
    frame.render_widget(Paragraph::new(if entries.is_empty() {
        app.text("Create a destination playlist first.", "Сначала создайте плейлист назначения.")
    } else if !editable {
        app.text("Select a playlist and press Edit playlist. Only this playlist will be unlocked.",
            "Выберите плейлист и нажмите «Редактировать». Откроется для правки только этот плейлист.")
    } else {
        app.text("Copy keeps files where they are. Move files removes tracks from the desk and moves them to the playlist folder.",
            "Копирование оставляет файлы на месте. Перенос убирает треки со стола и перемещает файлы в папку плейлиста.")
    }).style(palette.text()).wrap(Wrap { trim: false }), Rect::new(inner.x, inner.y, inner.width, hint_height));
    let capacity = inner.height.saturating_sub(footer + hint_height) as usize;
    let offset = visible_offset(0, selected, capacity, entries.len());
    for (index, (label, _, _)) in entries.iter().enumerate().skip(offset).take(capacity) {
        let rect = Rect::new(
            inner.x,
            inner.y + hint_height + (index - offset) as u16,
            inner.width,
            1,
        );
        let style = if index == selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        frame.render_widget(Paragraph::new(label.clone()).style(style), rect);
        app.view.hits.push(Hit {
            area: rect,
            target: Target::TransferRow(index),
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

pub(super) fn move_files(
    frame: &mut Frame,
    app: &mut App,
    view: &MoveFiles,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Move files", "Перенести файлы").into(),
        22,
        palette,
    );
    let saving = app.writing_tags();
    let mut actions = vec![(
        app.text(
            if view.confirm {
                "Move"
            } else {
                "Select folder"
            },
            if view.confirm {
                "Перенести"
            } else {
                "Выбрать папку"
            },
        )
        .into(),
        Target::Submit,
        !saving && view.folders.get(view.selected).is_some(),
    )];
    if !view.confirm {
        actions.push((
            app.text("Other folder", "Другая папка").into(),
            Target::MoveFilesFolder,
            !saving,
        ));
    }
    actions.push((
        app.text("Back", "Назад").into(),
        Target::CloseDialog,
        !saving,
    ));
    let footer = buttons_height(app, inner.width, &actions).min(inner.height);
    let content = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(footer),
    );
    if view.confirm || saving {
        let folder = view
            .folders
            .get(view.selected)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let description = if saving {
            app.text("Moving files…", "Перенос файлов…")
        } else {
            app.text("Move the selected files? Tracks will leave the sorting desk and appear at the start of this playlist. Paths in other playlists will also be updated.",
                "Перенести выбранные файлы? Треки уйдут со стола и появятся в начале этого плейлиста. Пути в других плейлистах тоже обновятся.")
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{} · {}: {}\n{}\n\n{description}",
                clean(&view.name),
                app.text("Files", "Файлов"),
                view.ids.len(),
                clean(&folder)
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            content,
        );
    } else {
        frame.render_widget(
            Paragraph::new(format!(
                "{}\n{}",
                clean(&view.name),
                if view.folders.is_empty() {
                    app.text(
                        "No linked folders. Choose Other folder.",
                        "Связанных папок нет. Нажмите «Другая папка».",
                    )
                } else {
                    app.text(
                        "Select the destination folder:",
                        "Выберите папку назначения:",
                    )
                }
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            Rect::new(content.x, content.y, content.width, 3.min(content.height)),
        );
        let capacity = content.height.saturating_sub(3) as usize;
        let offset = visible_offset(0, view.selected, capacity, view.folders.len());
        for (index, folder) in view.folders.iter().enumerate().skip(offset).take(capacity) {
            let rect = Rect::new(
                content.x,
                content.y + 3 + (index - offset) as u16,
                content.width,
                1,
            );
            let style = if index == view.selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            };
            frame.render_widget(
                Paragraph::new(clean(&folder.display().to_string())).style(style),
                rect,
            );
            app.view.hits.push(Hit {
                area: rect,
                target: Target::MoveFilesRow(index),
                enabled: true,
            });
        }
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
