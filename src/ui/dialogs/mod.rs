mod bindings;
mod browser;
mod equalizer;
mod help;
mod panels;
mod playlists;
mod search;
mod text;
pub(crate) use text::keyboard;

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
        Dialog::Equalizer {
            selected,
            presets,
            preset_selected,
        } => {
            equalizer::render(
                frame,
                app,
                selected,
                *presets,
                preset_selected,
                area,
                palette,
            );
        }
        Dialog::Commands {
            menu,
            selected,
            anchor,
        } => {
            super::menus::render(frame, app, *menu, selected, *anchor, palette);
        }
        Dialog::Panels { selected, expanded } => {
            panels::render(frame, app, selected, expanded, area, palette);
        }
        Dialog::Text(dialog) => text_dialog(frame, app, dialog, area, palette),
        Dialog::Search(search) => self::search::render(frame, app, search, area, palette),
        Dialog::Browser(browser) => browser_dialog(frame, app, browser, area, palette),
        Dialog::Folders { playlist, selected } => {
            playlists::folders(frame, app, *playlist, selected, area, palette)
        }
        Dialog::ComposePlaylists { selected, ids } => {
            playlists::compose(frame, app, selected, ids, area, palette)
        }
        Dialog::RemoveEntry { title, .. } => {
            let inner = modal(
                frame,
                app,
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
                app,
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
                app,
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
            let inner = modal(frame, app, area, title.clone(), 22, palette);
            help::scroll_text(frame, app, offset,
                Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(3)),
                format!("{description}\n\n{}", app.text("Tab: switch panel. Arrows: select/change. Enter: edit/apply. Esc: return to settings.", "Tab: сменить панель. Стрелки: выбор и значение. Enter: изменить/применить. Esc: вернуться в настройки.")),
                palette);
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::Help { offset } => {
            let inner = modal(
                frame,
                app,
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
            lines.push(app.text("Hold the waveform or progress bar to choose a position silently; release to seek and keep the previous play/pause state. Esc cancels dragging. Click or drag the divided volume bar; Up/Down in the player changes volume by 1%.", "Зажмите аудиоволну или дорожку для тихого выбора позиции; отпустите для перемотки с сохранением воспроизведения или паузы. Esc отменяет перетаскивание. Полоска громкости управляется мышью; стрелки вверх/вниз в плеере меняют громкость на 1%.").into());
            lines.push(app.text("Repeat cycles off → playlist → track. Track repeat restarts the finished track; manual Next still skips it. Shuffle chooses among all other tracks in the playing playlist, and Previous retraces playback history. Track repeat takes priority over shuffle. With shuffle and only one track, playback stops unless track repeat is enabled. These modes save in the active profile; their shortcuts are listed above.", "Повтор переключается: выключен → плейлист → трек. Повтор трека запускает завершённый трек сначала; кнопка следующего позволяет его пропустить. Случайное проигрывание выбирает из остальных треков воспроизводимого плейлиста, а Назад возвращает по истории. Повтор трека имеет приоритет. При случайном проигрывании единственного трека плеер останавливается, если не включён повтор трека. Режимы сохраняются в активном профиле; сочетания клавиш указаны выше.").into());
            lines.push(app.text("Equalizer: the EQ button or its shortcut opens ten bands (32 Hz–16 kHz) and preamp, each from −12 to +12 dB. Drag sliders or use the wheel in 1 dB steps; Left/Right and Tab select a band, Up/Down changes its gain. Space toggles the equalizer; Enter opens presets, 1–8 applies one, R resets gains. Changes affect the current track immediately and save in the profile. Manual adjustments display Custom. Preamp adjusts all frequencies together.", "Эквалайзер: кнопка EQ или её горячая клавиша открывает десять полос (32 Гц–16 кГц) и предусиление от −12 до +12 дБ. Тяните ползунки или меняйте их колесом по 1 дБ; ←→ и Tab выбирают полосу, ↑↓ меняют усиление. Space включает эквалайзер; Enter открывает пресеты, 1–8 выбирает пресет, R сбрасывает усиление. Изменения сразу действуют на текущий трек и сохраняются в профиле. При ручной настройке показан «Пользовательский». Предусиление меняет громкость всех частот вместе.").into());
            lines.push(app.text("Settings > General > Playback timeline chooses waveform or progress bar. Only one view is shown. The choice applies immediately in GUI and TUI and is saved in the active profile. Progress bar mode skips waveform analysis.", "Настройки > Общие > Дорожка проигрывания: аудиоволна или обычная полоса. Показывается один выбранный вид. Выбор сразу применяется в GUI и TUI и сохраняется в активном профиле. При обычной полосе анализ аудиоволны не запускается.").into());
            lines.push(app.text("[▶] and [‖] mark the playing playlist and track, including pause. [■] stops playback. The selected row is highlighted separately. Playlist track counts stay visible on the right; long names end with an ellipsis.", "[▶] и [‖] обозначают воспроизводящийся плейлист и композицию, включая паузу. [■] останавливает воспроизведение. Выбранная строка выделяется отдельно. Счетчики композиций всегда справа; длинные названия сокращаются с многоточием.").into());
            lines.push(app.text("Playlist numbers show their positions. Select a regular playlist; Ctrl+Up/Down moves it by one position. At least two regular playlists are required in the current mode. In Order, enable Edit first. The sorting desk stays first and cannot be reordered. Hold LMB on a regular playlist row, drag onto another regular playlist and release to move it there. The target row is highlighted. While dragging, use the wheel to reach hidden rows. Esc cancels the drag; dropping outside the list changes nothing. Playlist order is saved; Chaos supports undo/redo. Tracks and their positions remain unchanged.", "Номера плейлистов показывают их позиции. Выберите обычный плейлист; Ctrl+вверх/вниз перемещает его на одну позицию. В текущем режиме нужны хотя бы два обычных плейлиста. В Порядке сначала включите Правку. Сортировочный стол закреплен первым и не переставляется. Зажмите ЛКМ на строке обычного плейлиста, перенесите на другой обычный плейлист и отпустите. Строка назначения подсвечивается. При переносе колесо открывает строки за пределами списка. Esc отменяет перенос; отпускание вне списка ничего не меняет. Порядок плейлистов сохраняется; в Хаосе работает отмена/повтор. Композиции и их позиции не меняются.").into());
            lines.push(app.text("Search always accepts text, spaces, paste and Backspace in its query field; Ctrl+A selects the query. Up/Down selects tracks; Enter or double-click plays the selection without ending input. Tab switches sections. In playlists, Left/Right selects a row and Ctrl+Space toggles it; Ctrl+L selects all playlists, Ctrl+D clears them. Mouse checkboxes work independently of typing. Filter edits the current playlist header immediately. Enter keeps it, Esc restores it, x clears it. Keyboard buttons allow mouse-only typing.", "Поиск всегда принимает текст, пробелы, вставку и Backspace в строку запроса; Ctrl+A выделяет запрос. Вверх/вниз выбирает композицию; Enter или двойной клик запускает ее, сохраняя возможность ввода. Tab переключает разделы. В плейлистах влево/вправо выбирает строку, Ctrl+Space меняет ее отметку; Ctrl+L выбирает все плейлисты, Ctrl+D снимает отметки. Чекбоксы работают мышью независимо от ввода. Фильтр сразу меняет вид текущего плейлиста в строке Композиций. Enter сохраняет его, Esc возвращает прежний, x очищает. Кнопки клавиатуры позволяют вводить мышью.").into());
            help::help_dialog(frame, app, offset, inner, lines, palette);
        }

        Dialog::Metadata { track, offset } => {
            let inner = modal(
                frame,
                app,
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
            help::scroll_text(
                frame,
                app,
                offset,
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
                lines.join("\n"),
                palette,
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::CaptureBinding { index } => {
            binding_dialog(frame, app, *index, area, palette);
        }
    }
}
