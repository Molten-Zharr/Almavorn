use super::super::{
    theme::Palette,
    widgets::{buttons, buttons_height, clean, modal, visible_offset},
};
use crate::{
    app::{App, Hit, MoveFiles, Target, TransferView},
    model::PlaylistKind,
};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    widgets::{Paragraph, Wrap},
};
use std::collections::{HashMap, HashSet};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut TransferView,
    area: Rect,
    palette: Palette,
) {
    let confirming = view.confirm.is_some();
    let inner = modal(
        frame,
        app,
        area,
        app.text(
            if confirming {
                "Confirm copying tracks"
            } else {
                "Copy tracks to a playlist"
            },
            if confirming {
                "Подтвердить копирование композиций"
            } else {
                "Копировать композиции в плейлист"
            },
        )
        .into(),
        area.height,
        palette,
    ).inner(Margin::new(1, 1));
    if compact(frame, app, inner, palette) { return; }
    let source = app.playlists().iter().find(|p| p.id == view.source);
    let source_name = source
        .map(|p| format!("{} ({})", clean(p.display_name(app.settings.preferences.language)), p.mode.name(app.settings.preferences.language)))
        .unwrap_or_default();
    let source_entries: HashMap<_, _> = source.into_iter().flat_map(|p| &p.entries).map(|entry| (entry.track.id, entry)).collect();
    let tracks: Vec<_> = view
        .ids
        .iter()
        .filter_map(|id| source_entries.get(id))
        .map(|e| {
            (
                e.track.id,
                if e.track.artist.trim().is_empty() {
                    clean(&e.track.title)
                } else {
                    format!("{} - {}", clean(&e.track.artist), clean(&e.track.title))
                },
            )
        })
        .collect();
    let from_desk = source.is_some_and(|p| p.kind == PlaylistKind::SortingDesk);
    let destinations = app.transfer_destinations(view.source);
    view.selected = view.selected.min(destinations.len().saturating_sub(1));
    let destination = if let Some(id) = view.confirm {
        destinations.iter().copied().find(|p| p.id == id)
    } else {
        destinations.get(view.selected).copied()
    };
    let destination_name = destination
        .map(|p| format!("{} ({})", clean(p.display_name(app.settings.preferences.language)), p.mode.name(app.settings.preferences.language)))
        .unwrap_or_else(|| app.text("Choose a playlist", "Выберите плейлист").into());
    let protected = destination.is_some_and(|p| !app.playlist_editable(p));
    let can_move = from_desk && destination.is_some_and(|p| p.kind == PlaylistKind::Normal);
    let existing_ids: HashSet<_> = destination.into_iter().flat_map(|p| &p.entries).map(|entry| entry.track.id).collect();
    let existing = tracks.iter().filter(|(id, _)| existing_ids.contains(id)).count();
    let added = tracks.len().saturating_sub(existing);
    let entries: Vec<_> = destinations
        .iter()
        .map(|p| {
            format!(
                "{} · {}{}",
                clean(p.display_name(app.settings.preferences.language)),
                p.mode.name(app.settings.preferences.language),
                if app.playlist_editable(p) {
                    ""
                } else {
                    app.text(" · Protected", " · Защищен")
                }
            )
        })
        .collect();
    let ready = destination.is_some() && added > 0 && tracks.len() == view.ids.len() && !app.busy();
    let track_list = tracks
        .iter()
        .enumerate()
        .map(|(index, (id, title))| {
            format!(
                "{}. {title}{}",
                index + 1,
                if existing_ids.contains(id) {
                    app.text(
                        " (already in destination)",
                        " (уже есть в плейлисте назначения)",
                    )
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut actions = if confirming {
        vec![
            (app.text("Back", "Назад").into(), Target::CloseDialog, true),
            (
                format!(
                    "{} ({added})",
                    app.text("Copy", "Копировать")
                ),
                Target::TransferConfirm,
                ready,
            ),
            ("↑".into(), Target::DialogScroll(-5), view.offset > 0),
            ("↓".into(), Target::DialogScroll(5), true),
        ]
    } else {
        let mut actions = vec![(
            app.text("Copy…", "Копировать…")
                .into(),
            Target::Submit,
            ready,
        )];
        if can_move {
            if protected {
                actions.push((
                    app.text(
                        "Enable editing",
                        "Разрешить правку",
                    )
                    .into(),
                    Target::TransferEdit,
                    !app.busy(),
                ));
            } else {
                actions.push((
                    app.text("Move…", "Перенос…").into(),
                    Target::TransferMove,
                    !app.busy(),
                ));
            }
        }
        actions.push((
            app.text("Cancel", "Отмена").into(),
            Target::CloseDialog,
            true,
        ));
        actions
    };
    let footer = buttons_height(app, inner.width, &actions).min(inner.height);
    let mut header = format!(
        "{}: {source_name}\n{}: {}\n{}: {destination_name}\n{}: {added} · {}: {existing}",
        app.text("From playlist", "Из плейлиста"),
        app.text("Selected tracks", "Выбрано композиций"),
        tracks.len(),
        app.text("To playlist", "В плейлист"),
        app.text("Will add", "Будет добавлено"),
        app.text("Already there", "Уже есть")
    );
    if confirming {
        header.push_str(app.text("\n\nOnly playlist entries are copied. Original entries and audio files stay in place.", "\n\nКопируются записи в плейлист. Композиции в исходном плейлисте и аудиофайлы остаются на месте."));
        if protected {
            header.push_str(app.text("\nThe destination is protected. Confirm this addition only; editing stays off.", "\nПлейлист назначения защищен. Подтверждается только это добавление; режим правки останется выключен."));
        }
        header.push_str(&format!(
            "\n\n{}",
            app.text("Tracks to copy:", "Какие композиции копируем:")
        ));
    }
    let footer_y = inner.bottom().saturating_sub(footer);
    if confirming {
        let body = Rect::new(inner.x, inner.y, inner.width,
            footer_y.saturating_sub(inner.y + 1));
        scroll_text(frame, app, &mut view.offset, body, format!("{header}\n{track_list}"), palette);
        if let Some(action) = actions.last_mut() {
            action.2 = view.offset < app.view.dialog_scroll_max;
        }
    } else {
        let intro = format!("{}: {}\n{}: {}", app.text("From", "Откуда"), source_name,
            app.text("Selected tracks", "Выбрано композиций"), tracks.len());
        let paragraph = Paragraph::new(intro).style(palette.text()).wrap(Wrap { trim: false });
        let height = (paragraph.line_count(inner.width) as u16).min(4);
        frame.render_widget(paragraph, Rect::new(inner.x, inner.y, inner.width, height));
        let heading_y = inner.y + height + 1;
        frame.render_widget(Paragraph::new(app.text("Choose destination playlist", "Выберите плейлист назначения"))
            .style(palette.text().fg(palette.accent)), Rect::new(inner.x, heading_y, inner.width, 1));
        let detail = if entries.is_empty() {
            app.text("Create a destination playlist first.", "Сначала создайте плейлист назначения.").into()
        } else {
            format!("{}: {added} · {}: {existing}\n{}{}", app.text("Will add", "Добавится"),
                app.text("Already there", "Уже есть"),
                app.text("Copy entries keeps audio files in place.", "Копирование записей сохраняет файлы на месте."),
                if can_move && protected { app.text(" Enable editing to move files from the desk.",
                    " Для переноса файлов со стола разрешите правку плейлиста.") }
                else if can_move { app.text(" Move changes their folder and removes tracks from the desk.",
                    " Перенос меняет папку файлов и убирает композиции со стола.") } else { "" })
        };
        let paragraph = Paragraph::new(detail).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false });
        let detail_height = (paragraph.line_count(inner.width) as u16)
            .min(6).min(footer_y.saturating_sub(heading_y + 7));
        let detail_y = footer_y.saturating_sub(detail_height + 1);
        frame.render_widget(paragraph, Rect::new(inner.x, detail_y, inner.width, detail_height));
        let body = Rect::new(inner.x, heading_y + 2, inner.width, detail_y.saturating_sub(heading_y + 3));
        let row_height = if body.height >= 8 { 2 } else { 1 };
        let capacity = usize::from(body.height / row_height);
        app.view.dialog_page_size = capacity;
        let offset = visible_offset(0, view.selected, capacity, entries.len());
        for (index, label) in entries.iter().enumerate().skip(offset).take(capacity) {
            let row = Rect::new(body.x, body.y + (index - offset) as u16 * row_height, body.width, row_height);
            let style = if index == view.selected || app.hovered(row) {
                palette.text().bg(palette.selection).fg(palette.selection_text)
            } else { palette.text() };
            frame.render_widget(Paragraph::new(label.as_str()).style(style).wrap(Wrap { trim: false }), row);
            app.view.hits.push(Hit { area: row, target: Target::TransferRow(index), enabled: true });
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

pub(super) fn move_files(
    frame: &mut Frame,
    app: &mut App,
    view: &mut MoveFiles,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Move files", "Перенести файлы").into(),
        area.height,
        palette,
    ).inner(Margin::new(1, 1));
    if compact(frame, app, inner, palette) { return; }
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
    if view.confirm {
        actions.push(("↑".into(), Target::DialogScroll(-5), view.offset > 0 && !saving));
        actions.push(("↓".into(), Target::DialogScroll(5), !saving));
    }
    let footer = buttons_height(app, inner.width, &actions).min(inner.height);
    let content = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(footer + 1),
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
        let source = app.playlists().iter().find(|p| p.id == view.desk);
        let preview = view
            .ids
            .iter()
            .filter_map(|id| source?.entries.iter().find(|e| e.track.id == *id))
            .map(|e| format!("{} - {}", clean(&e.track.artist), clean(&e.track.title)))
            .collect::<Vec<_>>()
            .join("\n");
        scroll_text(frame, app, &mut view.offset, content, format!(
                "{}: {}\n{} · {}: {}\n{}\n\n{description}\n\n{preview}",
                app.text("From playlist", "Из плейлиста"),
                source
                    .map(|p| clean(p.display_name(app.settings.preferences.language)))
                    .unwrap_or_default(),
                clean(&view.name),
                app.text("Files", "Файлов"),
                view.ids.len(),
                clean(&folder)
            ), palette);
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
        let row_height = if content.width >= 48 { 2 } else { 3 };
        let capacity = usize::from(content.height.saturating_sub(4) / row_height);
        app.view.dialog_page_size = capacity;
        let offset = visible_offset(0, view.selected, capacity, view.folders.len());
        for (index, folder) in view.folders.iter().enumerate().skip(offset).take(capacity) {
            let rect = Rect::new(
                content.x,
                content.y + 4 + (index - offset) as u16 * row_height,
                content.width,
                row_height,
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
                Paragraph::new(clean(&folder.display().to_string())).style(style).wrap(Wrap { trim: false }),
                rect,
            );
            app.view.hits.push(Hit {
                area: rect,
                target: Target::MoveFilesRow(index),
                enabled: true,
            });
        }
    }
    if view.confirm && let Some(action) = actions.last_mut() {
        action.2 = !saving && view.offset < app.view.dialog_scroll_max;
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

fn compact(frame: &mut Frame, app: &mut App, inner: Rect, palette: Palette) -> bool {
    if inner.width >= 24 && inner.height >= 18 { return false; }
    frame.render_widget(Paragraph::new(app.text("Enlarge the window to choose a destination.",
        "Увеличьте окно для выбора назначения.")).style(palette.text()).wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2)));
    buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, inner.height.min(1)),
        vec![(app.text("Back", "Назад").into(), Target::CloseDialog, true)], palette);
    true
}

fn scroll_text(frame: &mut Frame, app: &mut App, offset: &mut usize, area: Rect, value: String, palette: Palette) {
    let paragraph = Paragraph::new(value).style(palette.text()).wrap(Wrap { trim: false });
    app.view.dialog_page_size = usize::from(area.height);
    app.view.dialog_scroll_max = paragraph.line_count(area.width.max(1))
        .saturating_sub(usize::from(area.height)).min(u16::MAX as usize);
    *offset = (*offset).min(app.view.dialog_scroll_max);
    frame.render_widget(paragraph.scroll((*offset as u16, 0)), area);
}
