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
    layout::Rect,
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
        32,
        palette,
    );
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
                    app.text("Confirm copying", "Подтвердить копирование")
                ),
                Target::TransferConfirm,
                ready,
            ),
            ("↑".into(), Target::DialogScroll(-5), view.offset > 0),
            ("↓".into(), Target::DialogScroll(5), true),
        ]
    } else {
        let mut actions = vec![(
            app.text("Next: review copy", "Далее: проверить копирование")
                .into(),
            Target::Submit,
            ready,
        )];
        if can_move {
            if protected {
                actions.push((
                    app.text(
                        "Enable editing for file move",
                        "Включить правку для переноса файлов",
                    )
                    .into(),
                    Target::TransferEdit,
                    !app.busy(),
                ));
            } else {
                actions.push((
                    app.text("Move files…", "Перенести файлы…").into(),
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
    } else {
        header.push_str(&format!(
            "\n{}: {}",
            app.text("Tracks", "Композиции"),
            tracks
                .iter()
                .take(2)
                .map(|(_, title)| title.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        ));
        if tracks.len() > 2 {
            header.push_str(&format!(
                " · {} {}",
                app.text("and", "и еще"),
                tracks.len() - 2
            ));
        }
        header.push_str(if entries.is_empty() {
            app.text("\n\nCreate a destination playlist first.", "\n\nСначала создайте плейлист назначения.")
        } else {
            app.text("\n\nChoose the destination. The next step shows the full track list for confirmation.", "\n\nВыберите плейлист назначения. На следующем шаге будет полный список и подтверждение.")
        });
    }
    let paragraph = Paragraph::new(header)
        .style(palette.text())
        .wrap(Wrap { trim: false });
    let header_height = (paragraph.line_count(inner.width.max(1)) as u16)
        .min(inner.height.saturating_sub(footer + 3));
    frame.render_widget(
        paragraph,
        Rect::new(inner.x, inner.y, inner.width, header_height),
    );
    let body = Rect::new(
        inner.x,
        inner.y + header_height,
        inner.width,
        inner.height.saturating_sub(footer + header_height),
    );
    if confirming {
        let paragraph = Paragraph::new(track_list)
            .style(palette.text())
            .wrap(Wrap { trim: false });
        app.view.dialog_page_size = body.height as usize;
        app.view.dialog_scroll_max = paragraph
            .line_count(body.width.max(1))
            .saturating_sub(body.height as usize)
            .min(u16::MAX as usize);
        view.offset = view.offset.min(app.view.dialog_scroll_max);
        frame.render_widget(paragraph.scroll((view.offset as u16, 0)), body);
        if let Some(action) = actions.last_mut() {
            action.2 = view.offset < app.view.dialog_scroll_max;
        }
    } else {
        let capacity = body.height as usize;
        app.view.dialog_page_size = capacity;
        let offset = visible_offset(0, view.selected, capacity, entries.len());
        for (index, label) in entries.iter().enumerate().skip(offset).take(capacity) {
            let row = Rect::new(body.x, body.y + (index - offset) as u16, body.width, 1);
            let style = if index == view.selected || app.hovered(row) {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            };
            frame.render_widget(Paragraph::new(label.as_str()).style(style), row);
            app.view.hits.push(Hit {
                area: row,
                target: Target::TransferRow(index),
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
        let source = app.playlists().iter().find(|p| p.id == view.desk);
        let preview = view
            .ids
            .iter()
            .take(3)
            .filter_map(|id| source?.entries.iter().find(|e| e.track.id == *id))
            .map(|e| format!("{} - {}", clean(&e.track.artist), clean(&e.track.title)))
            .collect::<Vec<_>>()
            .join("\n");
        frame.render_widget(
            Paragraph::new(format!(
                "{}: {}\n{} · {}: {}\n{}\n\n{preview}\n\n{description}",
                app.text("From playlist", "Из плейлиста"),
                source
                    .map(|p| clean(p.display_name(app.settings.preferences.language)))
                    .unwrap_or_default(),
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
