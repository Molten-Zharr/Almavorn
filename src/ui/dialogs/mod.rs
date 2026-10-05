mod browser;
mod text;

use self::{browser::browser_dialog, text::text_dialog};
use super::{
    theme::Palette,
    widgets::{buttons, dialog_footer, modal, visible_offset},
};
use crate::{
    app::{App, Dialog, Hit, Target},
    model::{Language, duration_text},
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render_dialog(
    frame: &mut Frame,
    app: &mut App,
    dialog: &mut Dialog,
    palette: Palette,
) {
    let mut area = frame.area();
    area.height = area.height.saturating_sub(3);
    match dialog {
        Dialog::Text(dialog) => text_dialog(frame, app, dialog, area, palette),
        Dialog::Browser(browser) => browser_dialog(frame, app, browser, area, palette),
        Dialog::RemoveEntry { title, .. } => {
            let inner = modal(
                frame,
                area,
                app.text("Remove track?", "Убрать композицию?").into(),
                10,
                palette,
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "{}\n\n{}",
                    title,
                    app.text(
                        "Remove from this playlist? The music file and saved tags are kept.",
                        "Убрать из этого плейлиста? Музыкальный файл и сохраненные теги останутся."
                    )
                ))
                .style(palette.text())
                .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (app.text("Remove", "Убрать").into(), Target::Submit, true),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::Transfer { selected, .. } => {
            let inner = modal(
                frame,
                area,
                app.text("Copy tracks to playlist", "Копировать в плейлист")
                    .into(),
                24,
                palette,
            );
            let entries: Vec<String> = app
                .transfer_destinations()
                .iter()
                .map(|playlist| {
                    format!(
                        "{} · {}",
                        playlist.display_name(app.settings.language),
                        playlist.mode.name(app.settings.language)
                    )
                })
                .collect();
            let capacity = inner.height.saturating_sub(3) as usize;
            let offset = visible_offset(0, *selected, capacity, entries.len());
            if entries.is_empty() {
                frame.render_widget(
                    Paragraph::new(app.text(
                        "Create a playlist or unlock Order editing.",
                        "Создайте плейлист или включите редактирование Порядка.",
                    ))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                    inner,
                );
            }
            for (row, label) in entries.iter().enumerate().skip(offset).take(capacity) {
                let rect = Rect::new(inner.x, inner.y + (row - offset) as u16, inner.width, 1);
                frame.render_widget(
                    Paragraph::new(label.clone()).style(if row == *selected {
                        palette.text().bg(palette.selection)
                    } else {
                        palette.text()
                    }),
                    rect,
                );
                app.hits.push(Hit {
                    area: rect,
                    target: Target::TransferRow(row),
                    enabled: true,
                });
            }
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (
                        app.text("Copy", "Копировать").into(),
                        Target::Submit,
                        !entries.is_empty(),
                    ),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::Settings { selected } => {
            let inner = modal(
                frame,
                area,
                app.text("Settings · current mode", "Настройки · текущий режим")
                    .into(),
                40,
                palette,
            );
            let theme = &app.settings.themes[app.settings.mode.index()];
            let mut rows = vec![
                format!(
                    "{}: {}",
                    app.text("Language", "Язык"),
                    if app.settings.language == Language::English {
                        "English"
                    } else {
                        "Русский"
                    }
                ),
                format!(
                    "{}: {}",
                    app.text("Theme", "Тема"),
                    app.text(
                        if theme.light { "Light" } else { "Dark" },
                        if theme.light {
                            "Светлая"
                        } else {
                            "Темная"
                        }
                    )
                ),
                format!(
                    "{}: #{:02X}{:02X}{:02X}",
                    app.text("Accent", "Акцент"),
                    theme.accent[0],
                    theme.accent[1],
                    theme.accent[2]
                ),
                format!(
                    "{}: {}",
                    app.text("Playlists panel", "Панель плейлистов"),
                    app.settings.playlist_placement.name(app.settings.language)
                ),
                format!(
                    "{}: {}",
                    app.text("Player panel", "Панель проигрывателя"),
                    app.settings.player_placement.name(app.settings.language)
                ),
                app.text("Reset shortcuts", "Сбросить горячие клавиши")
                    .into(),
            ];
            rows.extend(app.settings.bindings.iter().map(|binding| {
                format!(
                    "{} · {}",
                    binding.key.label(),
                    binding.action.name(app.settings.language)
                )
            }));
            let capacity = inner.height.saturating_sub(3) as usize;
            let offset = visible_offset(0, *selected, capacity, rows.len());
            for (row, label) in rows.iter().enumerate().skip(offset).take(capacity) {
                let rect = Rect::new(inner.x, inner.y + (row - offset) as u16, inner.width, 1);
                frame.render_widget(
                    Paragraph::new(label.clone()).style(if row == *selected {
                        palette.text().bg(palette.selection)
                    } else {
                        palette.text()
                    }),
                    rect,
                );
                app.hits.push(Hit {
                    area: rect,
                    target: Target::Setting(row),
                    enabled: true,
                });
            }
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::Help { offset } => {
            let inner = modal(
                frame,
                area,
                app.text("Guide and shortcuts", "Гайд и горячие клавиши")
                    .into(),
                44,
                palette,
            );
            let mut lines=vec![app.text("Mouse: click to select; double-click a track to play; scroll lists.","Мышь: клик — выбор; двойной клик по композиции — играть; колесо — прокрутка.").to_owned(),app.text("Use track checkboxes to select several tracks for copying.","Чекбоксы композиций отмечают несколько файлов для копирования.").into(),app.text("Keyboard: arrows select, Tab changes panel, Enter plays or confirms, Esc closes.","Клавиатура: стрелки — выбор, Tab — панель, Enter — играть или подтвердить, Esc — закрыть.").into(),String::new(),app.text("Order playlists are protected. Enable Edit to rename, remove, reorder or add directly.","Плейлисты Порядка защищены. Включите Правку для переименования, удаления, перемещения и прямого добавления.").into(),app.text("The sorting desk is always editable. Its checkbox routes every addition there.","Сортировочный стол всегда доступен для правки. Его чекбокс направляет туда все добавляемые файлы.").into(),app.text("Chaos and the desk support undo/redo in this session. Concurrent edits in the same scope may reset history. Only one app process can open this library for writing.","Хаос и стол поддерживают отмену/повтор в этой сессии. Одновременная правка тех же списков может очистить историю. Библиотеку для записи открывает только один процесс приложения.").into(),app.text("Sorting and searching change the view, never stored positions or source tags.","Сортировка и поиск меняют вид, а не сохраненные позиции или теги исходных файлов.").into(),app.text("Add new scans the folders of the current tracks; duplicates are skipped.","Добавить новые проверяет папки текущих композиций; повторные файлы пропускаются.").into(),format!("{}: {}",app.text("Database","База"),app.store.path.display()),String::new()];
            lines.extend(app.settings.bindings.iter().map(|binding| {
                format!(
                    "{} — {}",
                    binding.key.label(),
                    binding.action.name(app.settings.language)
                )
            }));
            *offset = (*offset).min(lines.len().saturating_sub(1));
            frame.render_widget(
                Paragraph::new(lines.join("\n"))
                    .scroll((*offset as u16, 0))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::Metadata { track, offset } => {
            let inner = modal(
                frame,
                area,
                app.text(
                    "Saved track information",
                    "Сохраненные сведения о композиции",
                )
                .into(),
                40,
                palette,
            );
            let mut lines = vec![
                format!("{}: {}", app.text("Title", "Название"), track.title),
                format!("{}: {}", app.text("Artist", "Исполнитель"), track.artist),
                format!("{}: {}", app.text("Album", "Альбом"), track.album),
                format!(
                    "{}: {}",
                    app.text("Duration", "Длительность"),
                    duration_text(track.duration_ms)
                ),
                format!("{}: {}", app.text("File", "Файл"), track.path.display()),
                String::new(),
                app.text(
                    "Original imported tags (read-only):",
                    "Исходные импортированные теги (только чтение):",
                )
                .into(),
            ];
            if let Ok(tags) =
                serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&track.tags)
            {
                lines.extend(
                    tags.into_iter()
                        .map(|(key, values)| format!("{key}: {}", values.join("; "))),
                );
            }
            *offset = (*offset).min(lines.len().saturating_sub(1));
            frame.render_widget(
                Paragraph::new(lines.join("\n"))
                    .scroll((*offset as u16, 0))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::CaptureBinding { index } => {
            let name = app
                .settings
                .bindings
                .get(*index)
                .map(|binding| binding.action.name(app.settings.language))
                .unwrap_or("");
            let inner = modal(
                frame,
                area,
                app.text("Set shortcut", "Назначить сочетание").into(),
                10,
                palette,
            );
            frame.render_widget(Paragraph::new(format!("{name}\n\n{}",app.text("Press a new shortcut. Esc cancels. Existing shortcuts and navigation keys are protected.","Нажмите новое сочетание. Esc — отмена. Занятые сочетания и клавиши навигации защищены."))).style(palette.text()).wrap(Wrap{trim:false}),inner);
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                vec![(
                    app.text("Cancel", "Отмена").into(),
                    Target::CloseDialog,
                    true,
                )],
                palette,
            );
        }
    }
}
