mod bindings;
mod browser;
mod help;
mod panels;
mod text;

use self::{bindings::binding_dialog, browser::browser_dialog, text::text_dialog};
use super::{
    theme::Palette,
    widgets::{button, buttons, dialog_footer, modal, visible_offset},
};
use crate::{
    app::{App, Dialog, Hit, Target},
    model::duration_text,
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
    area.height = area
        .height
        .saturating_sub(if dialog.is_settings() { 6 } else { 3 });
    if dialog.is_settings() && (area.width < 24 || area.height < 12) {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to edit this setting. Esc: back",
                "Увеличьте окно для редактирования. Esc: назад",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1)),
        );
        button(
            frame,
            app,
            Rect::new(
                area.x,
                area.bottom().saturating_sub(1),
                area.width,
                area.height.min(1),
            ),
            app.text("Back", "Назад"),
            Target::CloseDialog,
            true,
            palette,
        );
        return;
    }
    match dialog {
        Dialog::Panels { selected, expanded } => {
            panels::render(frame, app, selected, expanded, area, palette);
        }
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
                app.view.hits.push(Hit {
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
        Dialog::Settings { .. } => super::settings::render_settings(frame, app, palette),
        Dialog::ConfirmSettings { catalog, name, .. } => {
            let inner = modal(
                frame,
                area,
                app.text("Delete settings item?", "Удалить элемент настроек?")
                    .into(),
                14,
                palette,
            );
            let explanation = match catalog {
                crate::app::SettingsCatalog::Profile => app.text("The profile will be removed. If active, another profile is activated. Your music library is kept.", "Профиль будет удалён. Если он активен, включится другой. Музыкальная библиотека сохраняется."),
                crate::app::SettingsCatalog::Palette => app.text("The palette will be removed. Its themes and profiles use the first remaining palette.", "Палитра будет удалена. Использующие её темы и профили перейдут на первую оставшуюся палитру."),
                crate::app::SettingsCatalog::Preset => app.text("The preset will be removed. Your current appearance is kept.", "Пресет будет удалён. Текущее оформление сохраняется."),
            };
            frame.render_widget(
                Paragraph::new(format!(
                    "{name}\n\n{explanation}\n\n{}",
                    app.text(
                        "Enter: delete - Esc: cancel",
                        "Enter: удалить - Esc: отмена"
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
                    (app.text("Delete", "Удалить").into(), Target::Submit, true),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::SettingsHelp {
            title,
            description,
            offset,
        } => {
            let inner = modal(frame, area, title.clone(), 22, palette);
            frame.render_widget(Paragraph::new(format!("{description}\n\n{}", app.text("Tab: switch panel. Arrows: select/change. Enter: edit/apply. Esc: return to settings.", "Tab: сменить панель. Стрелки: выбор и значение. Enter: изменить/применить. Esc: вернуться в настройки."))).style(palette.text()).wrap(Wrap {trim:false}).scroll(((*offset).min(u16::MAX as usize) as u16, 0)), Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(3)));
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
            let mut lines=vec![app.text("Mouse: click to select; double-click a track to play; scroll lists.","Мышь: клик — выбор; двойной клик по композиции — играть; колесо — прокрутка.").to_owned(),app.text("Use track checkboxes to select several tracks for copying.","Чекбоксы композиций отмечают несколько файлов для копирования.").into(),String::new(),app.text("Order playlists are protected. Enable Edit to rename, remove, reorder or add directly.","Плейлисты Порядка защищены. Включите Правку для переименования, удаления, перемещения и прямого добавления.").into(),app.text("The sorting desk is always editable. Its checkbox routes every addition there.","Сортировочный стол всегда доступен для правки. Его чекбокс направляет туда все добавляемые файлы.").into(),app.text("Chaos and the desk support undo/redo in this session. Concurrent edits in the same scope may reset history. Only one app process can open this library for writing.","Хаос и стол поддерживают отмену/повтор в этой сессии. Одновременная правка тех же списков может очистить историю. Библиотеку для записи открывает только один процесс приложения.").into(),app.text("Sorting and searching change the view, never stored positions or source tags.","Сортировка и поиск меняют вид, а не сохраненные позиции или теги исходных файлов.").into(),app.text("Add new scans the folders of the current tracks; duplicates are skipped.","Добавить новые проверяет папки текущих композиций; повторные файлы пропускаются.").into(),format!("{}: {}",app.text("Database","База"),app.store.path.display()),String::new()];
            lines.push(app.text("Blocks: drag [↕] or a title; drop at an edge to dock, in the center to swap. Hold a shared border to resize. [-]/[+] collapses; [x] hides. Restore through Blocks. Expand a category there to choose visible buttons. Layout and visibility are saved in the active profile.", "Блоки: тяните [↕] или заголовок; край другого блока — разместить рядом, центр — поменять местами. Зажмите общую границу для размера. [-]/[+] сворачивает; [x] скрывает. Вернуть через меню «Блоки». Раскройте категорию для выбора видимых кнопок. Раскладка и видимость сохраняются в активный профиль.").into());
            lines.push(app.text("File picker: mark the files, then add the selection. Open folders to navigate.", "Выбор файлов: отметьте файлы, затем добавьте выбранные. Открывайте папки для перехода.").into());
            lines.push(
                app.text(
                    "Focus the player block to adjust volume and seek with the arrow keys.",
                    "Выберите блок проигрывателя для изменения громкости и перемотки стрелками.",
                )
                .into(),
            );
            lines.push(app.text("Click or drag the waveform and progress bar to seek. Click or drag the divided volume bar to adjust volume; Up/Down in the player changes it by 1%.", "Клик или перетаскивание по аудиоволне и дорожке меняет позицию. Полоска громкости с делениями управляется мышью; стрелки вверх/вниз в плеере меняют громкость на 1%.").into());
            lines.push(app.text("PLAYING and PAUSED mark the actual playing playlist and track. The selected row is highlighted separately.", "Метки ИГРАЕТ и ПАУЗА показывают воспроизводящийся плейлист и композицию. Выбранная строка выделяется отдельно.").into());
            help::help_dialog(frame, app, offset, inner, lines, palette);
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
            binding_dialog(frame, app, *index, area, palette);
        }
    }
}
