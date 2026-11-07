use super::super::{
    theme::Palette,
    widgets::{button_width, buttons, buttons_height, clean, file_modal, modal, visible_offset},
};
use crate::app::{App, Hit, Target};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
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
    let desk = playlist.kind == crate::model::PlaylistKind::SortingDesk;
    let editable = !desk && app.playlist_editable(playlist) && !app.busy();
    let paths: Vec<_> = playlist
        .scan_folders()
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let scan = !app.busy() && !paths.is_empty() && app.allowed(crate::input::Action::AddNew);
    let inner = file_modal(frame, app, area, clean(&title), 35, palette)
        .inner(Margin::new(1, 1));
    if inner.width < 28 || inner.height < if inner.width < 48 { 30 } else { 24 } {
        frame.render_widget(Paragraph::new(app.text("Enlarge the window to manage folders.",
            "Увеличьте окно для управления папками.")).style(palette.text()).wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2)));
        buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, inner.height.min(1)),
            vec![(app.text("Close", "Закрыть").into(), Target::CloseDialog, true)], palette);
        return;
    }
    *selected = (*selected).min(paths.len().saturating_sub(1));
    let intro = if desk {
        app.text("These folders belong to desk groups. Search adds new tracks to their groups.",
            "Это папки групп сортировочного стола. Поиск добавляет новые композиции в их группы.")
    } else {
        app.text("Link folders where you keep music. Search adds new tracks to this playlist and skips tracks already in it.",
            "Привяжите папки с музыкой. Поиск добавит новые композиции в этот плейлист, а уже добавленные пропустит.")
    };
    let paragraph = Paragraph::new(intro).style(palette.text()).wrap(Wrap { trim: false });
    let intro_height = (paragraph.line_count(inner.width) as u16).min(4);
    frame.render_widget(paragraph, Rect::new(inner.x, inner.y, inner.width, intro_height));
    let mut y = inner.y + intro_height + 1;
    if !desk {
        let attach = vec![(app.text("Link folder…", "Привязать папку…").into(), Target::FolderAdd, editable)];
        y += buttons(frame, app, Rect::new(inner.x, y, inner.width, 1), attach, palette) + 1;
    }
    frame.render_widget(Paragraph::new(format!("{}: {}", app.text("Folders", "Папок"), paths.len()))
        .style(palette.text().fg(palette.accent)), Rect::new(inner.x, y, inner.width, 1));
    y += 1;

    let footer_actions = vec![
        (app.text("Search all", "Искать во всех").into(), Target::FolderScan, scan),
        (app.text("Search selected", "В выбранной").into(), Target::FolderScanSelected, scan),
        (app.text("Close", "Закрыть").into(), Target::CloseDialog, true),
    ];
    let footer_height = buttons_height(app, inner.width, &footer_actions);
    let footer_y = inner.bottom() - footer_height;
    let search_hint = if desk {
        app.text("Only files directly in these folders are checked.", "На столе проверяются файлы непосредственно в папках групп.")
    } else if app.settings.preferences.scan_new_subfolders {
        app.text("Search includes nested folders. This setting also applies to other playlists.",
            "Поиск включает вложенные папки. Эта настройка действует и в остальных плейлистах.")
    } else {
        app.text("Search checks files directly in the linked folders.", "Поиск проверяет файлы непосредственно в связанных папках.")
    };
    let hint = Paragraph::new(search_hint).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false });
    let hint_height = (hint.line_count(inner.width) as u16).min(3);
    let search_y = footer_y - hint_height - 2;
    if !desk {
        buttons(frame, app, Rect::new(inner.x, search_y, inner.width, 1), vec![(
            format!("[{}] {}", if app.settings.preferences.scan_new_subfolders { 'x' } else { ' ' },
                app.text("Include subfolders", "С подпапками")), Target::ScanSubfolders, !app.busy())], palette);
    }
    frame.render_widget(hint, Rect::new(inner.x, search_y + 1, inner.width, hint_height));

    let context_actions = if desk { Vec::new() } else { vec![
        (app.text("Change folder…", "Другая папка…").into(), Target::FolderEdit, editable && !paths.is_empty()),
        (app.text("Unlink…", "Отвязать…").into(), Target::FolderRemove, editable && !paths.is_empty()),
    ] };
    let context_height = if desk { 0 } else { buttons_height(app, inner.width, &context_actions) };
    let context_y = search_y.saturating_sub(context_height + 1);
    let note = if desk {
        app.text("To remove a group, use its context menu on the desk.", "Убрать группу можно через ее меню на сортировочном столе.")
    } else if !editable && !app.busy() {
        app.text("Playlist protected: enable editing in the main window to change folder links.",
            "Плейлист защищен. Для изменения связей включите редактирование в основном окне.")
    } else {
        app.text("Replacing or unlinking a folder keeps music files and existing playlist tracks.",
            "Замена и отключение связи сохраняют файлы и уже добавленные композиции.")
    };
    let note = Paragraph::new(note).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false });
    let note_height = (note.line_count(inner.width) as u16).min(3);
    let note_y = context_y.saturating_sub(note_height + 1);
    frame.render_widget(note, Rect::new(inner.x, note_y, inner.width, note_height));
    let list = Rect::new(inner.x, y, inner.width, note_y.saturating_sub(y + 1));
    let row_height = if list.height >= 6 { 3 } else { 2 };
    let capacity = usize::from(list.height / row_height);
    app.view.dialog_page_size = capacity;
    let offset = visible_offset(0, *selected, capacity, paths.len());
    if paths.is_empty() {
        frame.render_widget(Paragraph::new(if desk {
            app.text("Add music to the desk to see its folders here.", "Добавьте музыку на стол, чтобы увидеть здесь папки групп.")
        } else {
            app.text("No linked folders yet. Choose Link folder, then Search all to add music.",
                "Связанных папок пока нет. Нажмите Привязать папку, затем Искать во всех, чтобы добавить музыку.")
        }).style(palette.text()).wrap(Wrap { trim: false }), list);
    }
    for (index, path) in paths.iter().enumerate().skip(offset).take(capacity) {
        let row = Rect::new(list.x, list.y + (index - offset) as u16 * row_height, list.width, row_height);
        let style = if index == *selected || app.hovered(row) {
            palette.text().bg(palette.selection).fg(palette.selection_text)
        } else { palette.text() };
        let name = std::path::Path::new(path).file_name().map(|name| name.to_string_lossy()).unwrap_or_else(|| path.into());
        frame.render_widget(Paragraph::new(format!("{}. {}", index + 1, clean(&name))).style(style),
            Rect::new(row.x, row.y, row.width, 1));
        frame.render_widget(Paragraph::new(clean(path)).style(style).wrap(Wrap { trim: false }),
            Rect::new(row.x, row.y + 1, row.width, row.height - 1));
        app.view.hits.push(Hit { area: row, target: Target::FolderRow(index), enabled: true });
    }
    buttons(frame, app, Rect::new(inner.x, context_y, inner.width, context_height), context_actions, palette);
    buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height), footer_actions, palette);
}

pub(super) fn unlink_folder(frame: &mut Frame, app: &mut App, path: &str, offset: &mut usize, area: Rect, palette: Palette) {
    let inner = file_modal(frame, app, area, app.text("Unlink folder?", "Отвязать папку?").into(),
        22, palette).inner(Margin::new(1, 1));
    let actions = vec![
        (app.text("Unlink", "Отвязать").into(), Target::Submit, !app.busy()),
        (app.text("Cancel", "Отмена").into(), Target::CloseDialog, true),
        ("↑".into(), Target::DialogScroll(-5), true),
        ("↓".into(), Target::DialogScroll(5), true),
    ];
    let height = buttons_height(app, inner.width, &actions).min(inner.height);
    let body = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(height + 1));
    super::help::scroll_text(frame, app, offset, body, format!("{}\n\n{}", clean(path), app.text(
        "This folder will no longer be searched for new music. Audio files stay on disk and existing tracks stay in the playlist.",
        "Эта папка больше не будет участвовать в поиске новых композиций. Аудиофайлы останутся на диске, а добавленные композиции останутся в плейлисте.")), palette);
    buttons(frame, app, Rect::new(inner.x, inner.bottom() - height, inner.width, height), actions, palette);
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
