use super::metadata_tag_name;
use crate::{
    app::{App, FileInput, Hit, MovePicker, TagAction, TagEdit, TagEditor, TagsTab, Target},
    tags::{field_keys, type_name},
    ui::{
        theme::Palette,
        widgets::{block, button, button_width, buttons, buttons_height, clean, file_modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::{Margin, Position, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

fn text(frame: &mut Frame, area: Rect, value: impl Into<String>, palette: Palette) {
    frame.render_widget(
        Paragraph::new(value.into())
            .style(palette.text())
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn action(label: &str, action: TagAction, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Tags(action), enabled)
}

fn checkbox(value: bool, label: &str) -> String {
    format!("[{}] {label}", if value { "x" } else { " " })
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    editor: &mut TagEditor,
    area: Rect,
    palette: Palette,
) {
    if (editor.confirm || editor.discard) && !app.writing_tags() {
        render_confirmation(frame, app, editor, area, palette);
        return;
    }
    let inner = file_modal(
        frame,
        app,
        area,
        if editor.is_batch() {
            format!("{} ({})", app.text("Tags of checked tracks", "Теги отмеченных композиций"), editor.batch.len())
        } else { app.text("Tags and file name", "Теги и имя файла").into() },
        46,
        palette,
    ).inner(Margin::new(1, 1));
    if inner.width < 24 || inner.height < if inner.width < 48 { 30 } else { 24 } {
        text(
            frame,
            Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(2)),
            app.text(
                "Enlarge the window. Esc: back",
                "Увеличьте окно. Esc: назад",
            ),
            palette,
        );
        buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, inner.height.min(1)),
            vec![action(app.text("Back", "Назад"), TagAction::Back, true)], palette);
        return;
    }
    let bottom = inner.bottom();
    let footer = Rect::new(inner.x, bottom - 3, inner.width, 3);
    if app.writing_tags() {
        text(
            frame,
            inner,
            app.text(
                "Saving files and updating the library…",
                "Сохранение файлов и обновление библиотеки…",
            ),
            palette,
        );
        return;
    }
    if let Some(report) = &editor.batch_report {
        let paragraph = Paragraph::new(report.as_str()).style(palette.text()).wrap(Wrap { trim: false });
        editor.batch_report_lines = paragraph.line_count(inner.width.max(1));
        editor.selected = editor.selected.min(editor.batch_report_lines.saturating_sub(1));
        frame.render_widget(paragraph.scroll((editor.selected.min(u16::MAX as usize) as u16, 0)),
            Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(3)));
        buttons(frame, app, footer, vec![action(app.text("Close", "Закрыть"), TagAction::Back, true)], palette);
        return;
    }
    let Some(document) = &editor.document else {
        text(
            frame,
            inner,
            editor
                .error
                .clone()
                .unwrap_or_else(|| app.text("Reading file tags…", "Чтение тегов файла…").into()),
            palette,
        );
        buttons(
            frame,
            app,
            footer,
            vec![action(app.text("Close", "Закрыть"), TagAction::Back, true)],
            palette,
        );
        return;
    };
    if let Some(edit) = &mut editor.edit
        && !editor.key_picker
    {
        render_edit(frame, app, edit, editor.error.as_deref(), inner, palette);
        return;
    }
    if editor.move_picker.is_some() {
        render_move(frame, app, editor, inner, palette);
        return;
    }
    let current = &document.blocks[editor.block];
    text(
        frame,
        Rect::new(inner.x, inner.y, inner.width, 1),
        if editor.is_batch() { format!("{}: {} · {}: {}", app.text("Checked", "Отмечено"), editor.batch.len(),
            app.text("Sample", "Образец"), clean(&editor.filename())) } else { format!(
            "{} · {}",
            clean(&editor.filename()),
            if editor.changed() { app.text("Unsaved changes", "Есть изменения") }
                else { app.text("No changes", "Без изменений") }
        ) },
        palette,
    );
    let mut tabs = vec![
        (TagsTab::Fields, app.text("Tags", "Теги")),
        (TagsTab::Covers, app.text("Covers", "Обложки")),
        (TagsTab::File, app.text("File name", "Имя файла")),
        (TagsTab::Extras, app.text("Extra", "Дополнительно")),
        (TagsTab::Cleanup, app.text("Cleanup", "Очистка")),
    ];
    tabs.retain(|(tab, _)| !editor.is_batch() || *tab != TagsTab::File);
    let tabs = tabs.into_iter().map(|(tab, label)| {
        (format!("{}{}", if editor.tab == tab { "● " } else { "" }, label),
            Target::Tags(TagAction::Tab(tab)), !editor.key_picker)
    }).collect::<Vec<_>>();
    let tab_height = buttons_height(app, inner.width, &tabs);
    buttons(frame, app, Rect::new(inner.x, inner.y + 2, inner.width, tab_height), tabs, palette);
    let mut y = inner.y + tab_height + 3;
    if editor.tab == TagsTab::File {
        render_file(
            frame,
            app,
            editor,
            Rect::new(inner.x, y, inner.width, bottom.saturating_sub(y)),
            palette,
        );
        return;
    }
    let formats = vec![
        action("◀", TagAction::Format(-1), editor.block > 0 && !editor.key_picker),
        (format!("{}: {}{}", app.text("Tag format", "Формат тегов"), type_name(current.kind),
            if current.existed { "" } else { app.text(" (new)", " (новый)") }),
            Target::Tags(TagAction::Format(1)), !editor.key_picker && editor.block + 1 < document.blocks.len()),
        action("▶", TagAction::Format(1), !editor.key_picker && editor.block + 1 < document.blocks.len()),
    ];
    {
        let height = buttons_height(app, inner.width, &formats);
        buttons(frame, app, Rect::new(inner.x, y, inner.width, height), formats, palette);
        y += height + 1;
    }
    let hint = if editor.is_batch() && !editor.key_picker {
        app.text("Values shown belong to the sample. Only edited fields and checked removals apply to all files.",
            "Показаны теги образца. Измененные поля и выбранная очистка применяются ко всем файлам.")
    } else if editor.key_picker {
        app.text(
            "Choose a field. Enter or double-click: add.",
            "Выберите поле. Enter или двойной клик: добавить.",
        )
    } else {
        match editor.tab {
            TagsTab::File => "",
            TagsTab::Fields | TagsTab::Extras => app.text(
                "Enter: edit. Clear selected marks a field for removal when saved.",
                "Enter: изменить. Очистка выбранного поля применяется при сохранении.",
            ),
            TagsTab::Covers => app.text(
                "Enter: edit. Clear selected marks a cover for removal when saved.",
                "Enter: изменить. Удаление выбранной обложки применяется при сохранении.",
            ),
            TagsTab::Cleanup => app.text(
                "Checked blocks will be removed from the file on Save.",
                "Отмеченные блоки будут удалены из файла после сохранения.",
            ),
        }
    };
    let hint_height = (Paragraph::new(hint).wrap(Wrap { trim: false }).line_count(inner.width) as u16).min(3);
    text(frame, Rect::new(inner.x, y, inner.width, hint_height), hint, Palette { text: palette.muted, ..palette });
    let error_height = if editor.error.is_some() { 3 } else { 0 };
    let footer_actions = vec![
        action(if editor.is_batch() { app.text("Save files", "Сохранить файлы") }
            else { app.text("Save file", "Сохранить файл") }, TagAction::Save, editor.changed() && !editor.key_picker),
        action(app.text("Close", "Закрыть"), TagAction::Back, true),
    ];
    let footer_height = buttons_height(app, inner.width, &footer_actions);
    let footer = Rect::new(inner.x, bottom - footer_height, inner.width, footer_height);
    let controls_height = if inner.width < 48 { 3 } else { 2 };
    let controls_y = footer.y.saturating_sub(controls_height + error_height + 2);
    let list = Rect::new(inner.x, y + hint_height + 1, inner.width,
        controls_y.saturating_sub(y + hint_height + 2));
    let keys = field_keys(current.kind);
    let length = editor.length();
    editor.selected = editor.selected.min(length.saturating_sub(1));
    let row_height = if list.width >= 48 && list.height >= 8 && !editor.key_picker { 2 } else { 1 };
    let capacity = usize::from(list.height / row_height);
    editor.offset = visible_offset(editor.offset, editor.selected, capacity, length);
    if length == 0 {
        text(
            frame,
            list,
            app.text("No entries. Choose Add.", "Пока пусто. Нажмите Добавить."),
            Palette {
                text: palette.muted,
                ..palette
            },
        );
    }
    for (line, index) in (editor.offset..length)
        .take(capacity)
        .enumerate()
    {
        let row = Rect::new(list.x, list.y + line as u16 * row_height, list.width, row_height);
        let (removed, value) = if editor.key_picker {
            let key = keys[index];
            (
                false,
                format!(
                    "{}  [{}]",
                    metadata_tag_name(app, &format!("{key:?}")),
                    key.map_key(current.kind).unwrap_or("")
                ),
            )
        } else {
            match editor.tab {
                TagsTab::File => (false, String::new()),
                TagsTab::Fields => {
                    let field = &current.fields[index];
                    (
                        field.remove || current.clear_fields || current.remove,
                        format!(
                            "{}: {}",
                            metadata_tag_name(app, &format!("{:?}", field.item.key())),
                            match field.item.value() {
                                lofty::tag::ItemValue::Binary(bytes) =>
                                    format!("{} {}", bytes.len(), app.text("bytes", "байт")),
                                lofty::tag::ItemValue::Text(value)
                                | lofty::tag::ItemValue::Locator(value) => clean(
                                    &value.chars().take(list.width as usize).collect::<String>()
                                ),
                            }
                        ),
                    )
                }
                TagsTab::Covers => {
                    let cover = &current.covers[index];
                    (
                        cover.remove || current.clear_covers || current.remove,
                        format!(
                            "#{} {:?} · {} · {} {}",
                            index + 1,
                            cover.picture.pic_type(),
                            clean(cover.picture.description().unwrap_or_default()),
                            cover.picture.data().len(),
                            app.text("bytes", "байт")
                        ),
                    )
                }
                TagsTab::Extras => {
                    let field = &current.extras[index];
                    (
                        field.remove || current.clear_fields || current.remove,
                        format!(
                            "{}{}: {}",
                            clean(&field.name),
                            if field.value_kind == 2 { " [HEX]" } else { "" },
                            if field.value_kind == 2 {
                                format!("{} {}", field.value.len() / 2, app.text("bytes", "байт"))
                            } else {
                                clean(
                                    &field
                                        .value
                                        .chars()
                                        .take(list.width as usize)
                                        .collect::<String>(),
                                )
                            }
                        ),
                    )
                }
                TagsTab::Cleanup => {
                    let block = &document.blocks[index];
                    (
                        block.remove,
                        format!(
                            "{} · {} {} · {} {}",
                            type_name(block.kind),
                            block.fields.len(),
                            app.text("fields", "полей"),
                            block.covers.len(),
                            app.text("covers", "обложек")
                        ),
                    )
                }
            }
        };
        let style = if index == editor.selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        let value = if editor.tab == TagsTab::Cleanup && !editor.key_picker {
            checkbox(removed, &value)
        } else if removed {
            format!("{}: {value}", app.text("Will remove", "Удалится"))
        } else { value };
        if matches!(editor.tab, TagsTab::Fields | TagsTab::Extras) && row.width >= 48 && !editor.key_picker {
            let (label, value) = value.split_once(": ").unwrap_or((&value, ""));
            let label_width = (row.width / 3).min(26);
            frame.render_widget(Paragraph::new(label).style(style).wrap(Wrap { trim: false }),
                Rect::new(row.x, row.y, label_width, row.height));
            frame.render_widget(Paragraph::new(value).style(style).wrap(Wrap { trim: false }),
                Rect::new(row.x + label_width + 2, row.y, row.width - label_width - 2, row.height));
        } else {
            frame.render_widget(Paragraph::new(value).style(style).wrap(Wrap { trim: false }), row);
        }
        app.view.hits.push(Hit { area: row, target: Target::Tags(TagAction::Row(index)), enabled: true });
        if editor.tab == TagsTab::Cleanup && !editor.key_picker {
            app.view.hits.push(Hit { area: Rect::new(row.x, row.y, row.width.min(3), row.height),
                target: Target::Tags(TagAction::ClearBlock(index)), enabled: true });
        }
    }
    let controls = Rect::new(inner.x, controls_y, inner.width, controls_height);
    if editor.key_picker {
        buttons(
            frame,
            app,
            controls,
            vec![
                action(
                    app.text("Choose", "Выбрать"),
                    TagAction::ChooseKey(editor.selected),
                    !keys.is_empty(),
                ),
                action(app.text("Back", "Назад"), TagAction::Back, true),
            ],
            palette,
        );
    } else if editor.tab == TagsTab::Cleanup {
        let all = document.blocks.iter().all(|block| block.remove);
        buttons(
            frame,
            app,
            controls,
            vec![
                action(
                    &checkbox(all, app.text("Remove all tags", "Удалить все теги")),
                    TagAction::ClearAll,
                    true,
                ),
                action(
                    &checkbox(
                        current.clear_fields,
                        app.text("Clear fields", "Очистить поля"),
                    ),
                    TagAction::ClearFields,
                    current.writable,
                ),
                action(
                    &checkbox(
                        current.clear_covers,
                        app.text("Clear covers", "Удалить обложки"),
                    ),
                    TagAction::ClearCovers,
                    current.writable,
                ),
            ],
            palette,
        );
    } else {
        buttons(
            frame,
            app,
            controls,
            vec![
                action(
                    app.text("Add", "Добавить"),
                    TagAction::Add,
                    current.writable && !current.remove,
                ),
                action(
                    app.text("Edit", "Изменить"),
                    TagAction::Edit,
                    length > 0 && current.writable && !current.remove,
                ),
                action(
                    app.text("Clear selected", "Очистить выбранное"),
                    TagAction::Remove(editor.selected),
                    length > 0,
                ),
            ],
            palette,
        );
    }
    if let Some(error) = &editor.error {
        text(
            frame,
            Rect::new(
                inner.x,
                footer.y - error_height - 1,
                inner.width,
                error_height,
            ),
            error.clone(),
            Palette {
                text: palette.error,
                ..palette
            },
        );
    }
    buttons(frame, app, footer, footer_actions, palette);
}

fn render_confirmation(frame: &mut Frame, app: &mut App, editor: &mut TagEditor, area: Rect, palette: Palette) {
    let mut lines = Vec::<Line<'static>>::new();
    let heading = |value: &str| Line::from(Span::styled(value.to_owned(), palette.text().fg(palette.accent)));
    let title;
    let mut controls;
    if editor.discard {
        title = app.text("Discard changes?", "Отменить изменения?");
        lines.push(Line::from(clean(&editor.filename())));
        lines.push(Line::default());
        lines.push(Line::from(app.text("The unsaved draft will be discarded. The audio file stays unchanged.",
            "Несохраненный черновик будет отменен. Аудиофайл останется без изменений.")));
        controls = vec![
            action(app.text("Discard changes", "Отменить изменения"), TagAction::Discard, true),
            action(app.text("Keep editing", "Продолжить правку"), TagAction::Back, true),
        ];
    } else {
        title = if editor.move_confirm { app.text("Move file?", "Перенести файл?") }
            else { app.text("Save changes?", "Сохранить изменения?") };
        if editor.is_batch() {
            lines.push(heading(&format!("{}: {}", app.text("Files", "Файлов"), editor.batch.len())));
            lines.push(Line::from(app.text("Edited fields and selected removals apply to every checked file. Other tags keep their values.",
                "Измененные поля и выбранная очистка применятся ко всем отмеченным файлам. Остальные теги сохранят свои значения.")));
        } else {
            lines.push(heading(app.text("Before", "Было")));
            lines.push(Line::from(clean(&editor.track.path.display().to_string())));
            lines.push(Line::default());
            lines.push(heading(app.text("After saving", "Будет")));
            lines.push(Line::from(editor.target_path().map(|p| clean(&p.display().to_string()))
                .unwrap_or_else(|error| error.to_string())));
            if editor.move_confirm {
                lines.push(Line::default());
                lines.push(heading(&format!("{}: {}", app.text("Playlist", "Плейлист"),
                    editor.destination().map(|p| clean(&p.name)).unwrap_or_default())));
                lines.push(Line::from(app.text("The file moves to the chosen folder. The track leaves the desk and appears first in the playlist.",
                    "Файл переместится в выбранную папку. Композиция уйдет со стола и станет первой в плейлисте.")));
            }
        }
        lines.push(Line::default());
        lines.push(Line::from(app.text("Changes affect the file on disk and every playlist using it. Playlist undo cannot restore the file.",
            "Изменения затронут файл на диске и все плейлисты, где он используется. Отмена действий с плейлистом не восстановит файл.")));
        if editor.destructive() {
            lines.push(Line::from(Span::styled(app.text("Marked tags and covers will be removed from the file.",
                "Отмеченные теги и обложки будут удалены из файла."), palette.text().fg(palette.error))));
        }
        if editor.is_batch() {
            lines.push(Line::default());
            lines.extend(editor.batch.iter().map(|track| Line::from(clean(&track.path.display().to_string()))));
        }
        controls = vec![
            action(if editor.move_confirm { app.text("Move file", "Перенести файл") }
                else if editor.is_batch() { app.text("Save files", "Сохранить файлы") }
                else { app.text("Save file", "Сохранить файл") }, TagAction::Confirm, !app.busy()),
            action(app.text("Edit again", "К правке"), TagAction::Back, true),
        ];
    }
    if let Some(error) = &editor.error {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(error.clone(), palette.text().fg(palette.error))));
    }
    let width = area.width.saturating_sub(4).min(88);
    let measured = block(String::new(), palette, true).inner(Rect::new(0, 0, width, 46)).inner(Margin::new(1, 1));
    let paragraph = Paragraph::new(lines).style(palette.text()).wrap(Wrap { trim: false });
    let line_count = paragraph.line_count(measured.width.max(1));
    let (_, mut footer_height) = file_button_layout(app, measured.width, &controls);
    let available_height = area.height.min(46);
    let mut height = (line_count.min(u16::MAX as usize) as u16).saturating_add(footer_height + 5);
    let scrolling = height > available_height;
    if scrolling {
        controls.push(("↑".into(), Target::DialogScroll(-5), true));
        controls.push(("↓".into(), Target::DialogScroll(5), true));
        footer_height = file_button_layout(app, measured.width, &controls).1;
        height = available_height;
    }
    let inner = file_modal(frame, app, area, title.into(), height, palette).inner(Margin::new(1, 1));
    if inner.height < footer_height + 2 || inner.width < 20 {
        text(frame, Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(1)),
            app.text("Enlarge the window to review changes. Esc: back.", "Увеличьте окно для просмотра изменений. Esc: назад."), palette);
        buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, inner.height.min(1)),
            vec![action(app.text("Back", "Назад"), TagAction::Back, true)], palette);
        return;
    }
    let footer_y = inner.bottom() - footer_height;
    let body = Rect::new(inner.x, inner.y, inner.width, footer_y.saturating_sub(inner.y + 1));
    app.view.dialog_page_size = usize::from(body.height);
    app.view.dialog_scroll_max = line_count.saturating_sub(usize::from(body.height)).min(u16::MAX as usize);
    editor.offset = editor.offset.min(app.view.dialog_scroll_max);
    if scrolling {
        let count = controls.len();
        controls[count - 2].2 = editor.offset > 0;
        controls[count - 1].2 = editor.offset < app.view.dialog_scroll_max;
    }
    frame.render_widget(paragraph.scroll((editor.offset as u16, 0)), body);
    file_buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height), controls, palette);
}

fn render_edit(
    frame: &mut Frame,
    app: &mut App,
    edit: &mut TagEdit,
    error: Option<&str>,
    inner: Rect,
    palette: Palette,
) {
    let title = if let Some(input) = edit.file_input {
        match input {
            FileInput::Name => app.text("File name without extension", "Имя файла без расширения"),
            FileInput::Template => app.text("File name template", "Шаблон имени файла"),
            FileInput::Folder => app.text("Destination folder", "Папка назначения"),
        }
        .into()
    } else if edit.extra {
        app.text("Additional field", "Дополнительное поле").into()
    } else if edit.cover {
        app.text("Cover image", "Обложка").into()
    } else {
        metadata_tag_name(app, &format!("{:?}", edit.key))
    };
    text(
        frame,
        Rect::new(inner.x, inner.y, inner.width, 1),
        title,
        Palette {
            text: palette.accent,
            ..palette
        },
    );
    let kind = match edit.value_kind {
        0 => app.text("Text", "Текст"),
        1 => "URL",
        _ => "HEX",
    };
    let hint = if edit.file_input == Some(FileInput::Template) {
        app.text("{filename}: original name\n{title}: title; {artist}: artist\n{album}: album; {track}: track number\nThe template uses current tag values, including fields marked for removal.",
                "{filename}: прежнее имя\n{title}: название; {artist}: исполнитель\n{album}: альбом; {track}: номер композиции\nБерутся текущие значения тегов, включая поля, отмеченные для очистки.")
    } else if edit.file_input == Some(FileInput::Name) {
        app.text("Enter the name only. The audio extension stays unchanged. Enter: apply, Ctrl+A: select all.",
                "Введите только имя. Расширение аудиофайла сохранится. Enter: применить, Ctrl+A: выделить все.")
    } else if edit.file_input == Some(FileInput::Folder) {
        app.text(
            "Enter the full path to an existing folder. Enter: apply, Ctrl+A: select all.",
            "Введите полный путь к существующей папке. Enter: применить, Ctrl+A: выделить все.",
        )
    } else if edit.cover {
        app.text(
            "Image path: leave empty to keep it. Type: 3 front, 4 back, 0–20 ID3 types.",
            "Путь к картинке: пустой сохраняет прежнюю. Тип: 3 лицевая, 4 оборот, 0–20 типы ID3.",
        )
    } else {
        app.text(
            "Enter: apply to draft. Ctrl+A: select all. F2: details. F10: newline.",
            "Enter: в черновик. Ctrl+A: выделить все. F2: подробнее. F10: новая строка.",
        )
    };
    let hint_height = if edit.file_input == Some(FileInput::Template) {
        (Paragraph::new(hint).wrap(Wrap { trim: false }).line_count(inner.width.max(1)) as u16)
            .min(inner.height.saturating_sub(12))
    } else { 3 };
    text(frame, Rect::new(inner.x, inner.y + 1, inner.width, hint_height), hint,
        Palette { text: palette.muted, ..palette });
    let titles = if edit.cover {
        [
            app.text("Image path", "Путь к изображению"),
            app.text("Description", "Описание"),
            app.text("Picture type", "Тип обложки"),
        ]
    } else if edit.extra {
        [
            kind,
            app.text("Field name", "Название поля"),
            app.text("Native file format", "Формат исходного файла"),
        ]
    } else {
        [
            kind,
            app.text("Description", "Описание"),
            app.text("Language (rus, eng)", "Язык (rus, eng)"),
        ]
    };
    let simple = edit.file_input.is_none() && !edit.cover && !edit.extra;
    let inputs = if edit.file_input.is_some() || (simple && !edit.details) { 1 }
        else if edit.extra { 2 } else { 3 };
    let mut footer_actions: Vec<_> = vec![
            action(app.text("Apply to draft", "Внести в черновик"), TagAction::Apply, true),
            action(if edit.details { app.text("Hide details [F2]", "Скрыть детали [F2]") }
                else { app.text("Details [F2]", "Подробнее [F2]") }, TagAction::Details, true),
            action(app.text("Back", "Назад"), TagAction::Back, true),
        ].into_iter().filter(|(_, target, _)| simple || !matches!(target, Target::Tags(TagAction::Details))).collect();
    footer_actions.extend(super::super::widgets::text_history_controls(app, &edit.history[edit.input]));
    let (_, footer_height) = file_button_layout(app, inner.width, &footer_actions);
    let footer_y = inner.bottom().saturating_sub(footer_height);
    if inner.height < 1 + hint_height + inputs * 3 + footer_height + 2 {
        text(frame, Rect::new(inner.x, inner.y, inner.width, footer_y.saturating_sub(inner.y)),
            app.text("Enlarge the window to edit this field. Esc: back.", "Увеличьте окно для редактирования поля. Esc: назад."), palette);
        file_buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height.min(inner.height)), footer_actions, palette);
        return;
    }
    let value_height = if edit.file_input.is_some() { 3 } else {
        inner.height.saturating_sub(hint_height + footer_height + 4 + (inputs - 1) * 3).max(3)
    };
    let mut y = inner.y + 1 + hint_height;
    for (index, title) in titles
        .iter()
        .take(usize::from(inputs))
        .enumerate()
    {
        let height = if index == 0 { value_height } else { 3 };
        let area = Rect::new(inner.x, y, inner.width, height);
        let border = block((*title).into(), palette, edit.input == index);
        let content = border.inner(area);
        frame.render_widget(border, area);
        let value = &edit.values[index];
        let cursor_byte = value
            .char_indices()
            .nth(edit.cursor)
            .map_or(value.len(), |(i, _)| i);
        let before = &value[..cursor_byte];
        let width = usize::from(content.width.max(1));
        // Use the same untrimmed wrapping as Paragraph for multiline values.
        let cursor_line = Paragraph::new(before.to_owned())
            .wrap(Wrap { trim: false })
            .line_count(content.width.max(1))
            .saturating_sub(1);
        let scroll = if edit.input == index {
            cursor_line.saturating_sub(content.height.saturating_sub(1) as usize)
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(value.clone())
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                .style(if edit.input == index && edit.selected_all {
                    palette.text().bg(palette.selection)
                } else {
                    palette.text()
                }),
            content,
        );
        if edit.input == index && content.width > 0 && content.height > 0 {
            let column = before
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                % width;
            frame.set_cursor_position(Position::new(
                content.x + column as u16,
                content.y
                    + cursor_line
                        .saturating_sub(scroll)
                        .min(content.height as usize - 1) as u16,
            ));
        }
        app.view.hits.push(Hit {
            area,
            target: Target::Tags(TagAction::Input(index)),
            enabled: true,
        });
        y += height;
    }
    if let Some(error) = error {
        let remaining = footer_y.saturating_sub(y);
        let error_area = if remaining > 1 {
            Rect::new(inner.x, y, inner.width, remaining)
        } else {
            Rect::new(inner.x, inner.y + 1, inner.width, 3)
        };
        text(
            frame,
            error_area,
            error.to_owned(),
            Palette {
                text: palette.error,
                ..palette
            },
        );
    } else if simple && edit.details && y < footer_y {
        button(
            frame,
            app,
            Rect::new(inner.x, y, inner.width, 1),
            &format!("{}: {kind}", app.text("Value type", "Тип значения")),
            Target::Tags(TagAction::ValueKind),
            true,
            palette,
        );
    }
    file_buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height), footer_actions, palette);
}

// This tab uses full-height buttons so actions cannot be mistaken for body text.
fn file_button_layout(app: &App, width: u16, entries: &[(String, Target, bool)]) -> (Vec<Rect>, u16) {
    let gap = if width < 48 { 1 } else { 2 };
    let padding = if width < 48 { 0 } else { 4 };
    let mut x = 0;
    let mut y = 0;
    let mut rects = Vec::new();
    for (label, target, _) in entries {
        let size = button_width(app, label, target).saturating_add(padding).min(width);
        if x > 0 && x + size > width { x = 0; y += 4; }
        rects.push(Rect::new(x, y, size, 3));
        x += size + gap;
    }
    (rects, y + 3)
}

fn file_buttons(frame: &mut Frame, app: &mut App, area: Rect, entries: Vec<(String, Target, bool)>, palette: Palette) {
    let (rects, _) = file_button_layout(app, area.width, &entries);
    for ((label, target, enabled), relative) in entries.into_iter().zip(rects) {
        if relative.bottom() > area.height { break; }
        let rect = Rect::new(area.x + relative.x, area.y + relative.y, relative.width, 3);
        let colors = if enabled && matches!(target, Target::Tags(TagAction::Save | TagAction::Confirm)) {
            Palette { panel: palette.confirm_background, ..palette }
        } else { palette };
        button(frame, app, rect, &label, target, enabled, colors);
    }
}

fn render_file(
    frame: &mut Frame,
    app: &mut App,
    editor: &TagEditor,
    inner: Rect,
    palette: Palette,
) {
    let narrow = inner.width < 48;
    let footer_actions = vec![
        action(app.text("Save", "Сохранить"), TagAction::Save, editor.changed()),
        action(app.text("Move… [M]", "Перенести… [M]"), TagAction::Move, true),
        action(app.text("Close", "Закрыть"), TagAction::Back, true),
    ];
    let (_, footer_height) = file_button_layout(app, inner.width, &footer_actions);
    let name_actions = vec![action(app.text("Edit [Enter]", "Изменить [Enter]"), TagAction::FileInput(FileInput::Name), true)];
    let template_actions = vec![
        action(app.text("Edit [T]", "Изменить [T]"), TagAction::FileInput(FileInput::Template), true),
        action(if narrow { app.text("Build [G]", "Собрать [G]") }
            else { app.text("Build name [G]", "Собрать имя [G]") }, TagAction::FormatName, true),
    ];
    let side_by_side = inner.width >= 72;
    let actions_width = if side_by_side {
        let (rects, _) = file_button_layout(app, inner.width, &template_actions);
        rects.last().map_or(0, |rect| rect.right()).max(button_width(app, &name_actions[0].0, &name_actions[0].1) + 4)
    } else { inner.width };
    let field_width = if side_by_side { inner.width.saturating_sub(actions_width + 2) } else { inner.width };
    let (_, name_buttons_height) = file_button_layout(app, actions_width, &name_actions);
    let (_, template_buttons_height) = file_button_layout(app, actions_width, &template_actions);
    let proposed_name = editor.new_filename().unwrap_or_else(|_| editor.file_stem.clone());
    let name_field_height = (Paragraph::new(clean(&proposed_name)).wrap(Wrap { trim: false })
        .line_count(field_width.saturating_sub(2).max(1)).min(255) as u16).saturating_add(2).max(3);
    let name_height = if side_by_side { name_field_height.max(name_buttons_height) }
        else { name_field_height + 1 + name_buttons_height };
    let template_height = if side_by_side { 3.max(template_buttons_height) } else { 4 + template_buttons_height };
    let hint_height = if narrow { 1 } else { 2 };
    let needed = name_height + template_height + 1 + hint_height + 1 + footer_height;
    if inner.height < needed {
        text(frame, Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(3)),
            app.text("Enlarge the window to show file names and action buttons. Enter: name; T: template; G: build name.",
                "Увеличьте окно, чтобы видеть имена и кнопки. Enter: имя; T: шаблон; G: собрать имя."), palette);
        file_buttons(frame, app, Rect::new(inner.x, inner.bottom().saturating_sub(3), inner.width, inner.height.min(3)),
            vec![action(app.text("Close", "Закрыть"), TagAction::Back, true)], palette);
        return;
    }
    let footer_y = inner.bottom() - footer_height;
    let mut y = inner.y;
    if inner.height >= needed + 2 {
        text(frame, Rect::new(inner.x, y, inner.width, 1),
            format!("{}: {}", app.text("Current file", "Сейчас"), clean(&editor.filename())),
            Palette { text: palette.muted, ..palette });
        y += 2;
    }
    let sections = [
        (app.text("Name after saving", "Имя после сохранения"),
            proposed_name, FileInput::Name, name_actions, name_height, name_field_height),
        (app.text("File name template", "Шаблон имени"),
            editor.name_template.clone(), FileInput::Template, template_actions, template_height, 3),
    ];
    for (title, value, input, actions, height, field_height) in sections {
        let field = Rect::new(inner.x, y, field_width, field_height);
        let outer = block(title.into(), palette, app.hovered(field));
        let content = outer.inner(field);
        frame.render_widget(outer, field);
        text(frame, content, clean(&value), palette);
        app.view.hits.push(Hit { area: field, target: Target::Tags(TagAction::FileInput(input)), enabled: true });
        let controls = if side_by_side {
            Rect::new(field.right() + 2, y, actions_width, height)
        } else { Rect::new(inner.x, y + field_height + 1, inner.width, height - field_height - 1) };
        file_buttons(frame, app, controls, actions, palette);
        y += height + 1;
    }
    let hint = editor.error.as_deref().unwrap_or_else(|| if narrow {
        app.text("Changes are saved with Save.", "Запись изменений: Сохранить.")
    } else {
        app.text("Build name fills the name from the template. Save writes the name and tags to the file. Move also removes the track from the desk.",
            "Собрать имя подставляет теги в шаблон. Сохранить записывает имя и теги в файл. Перенос также убирает композицию со стола.")
    });
    text(frame, Rect::new(inner.x, y.min(footer_y), inner.width, footer_y.saturating_sub(y)), hint,
        Palette { text: if editor.error.is_some() { palette.error } else { palette.muted }, ..palette });
    file_buttons(frame, app, Rect::new(inner.x, footer_y, inner.width, footer_height), footer_actions, palette);
}

fn render_move(
    frame: &mut Frame,
    app: &mut App,
    editor: &mut TagEditor,
    inner: Rect,
    palette: Palette,
) {
    let folders = editor.move_picker == Some(MovePicker::Folders);
    text(
        frame,
        Rect::new(inner.x, inner.y, inner.width, 2),
        if folders {
            format!(
                "{}: {}",
                app.text("Folder for playlist", "Папка плейлиста"),
                editor
                    .destination()
                    .map(|p| p.name.as_str())
                    .unwrap_or_default()
            )
        } else {
            app.text(
                "Choose a playlist for the file",
                "Выберите плейлист для переноса файла",
            )
            .into()
        },
        palette,
    );
    let footer = Rect::new(inner.x, inner.bottom() - 3, inner.width, 3);
    let list = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(12),
    );
    let length = editor.length();
    editor.selected = editor.selected.min(length.saturating_sub(1));
    editor.offset = visible_offset(editor.offset, editor.selected, list.height as usize, length);
    if length == 0 {
        text(
            frame,
            list,
            app.text(
                "No linked folders. Choose Enter folder.",
                "Связанных папок нет. Нажмите Указать папку.",
            ),
            palette,
        );
    }
    for (line, index) in (editor.offset..length)
        .take(list.height as usize)
        .enumerate()
    {
        let value = if folders {
            editor.destination().unwrap().folders[index]
                .display()
                .to_string()
        } else {
            let destination = &editor.destinations[index];
            format!(
                "{} · {}",
                destination.name,
                destination.mode.name(app.settings.preferences.language)
            )
        };
        let row = Rect::new(list.x, list.y + line as u16, list.width, 1);
        frame.render_widget(
            Paragraph::new(clean(&value)).style(if editor.selected == index {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            }),
            row,
        );
        app.view.hits.push(Hit {
            area: row,
            target: Target::Tags(TagAction::Row(index)),
            enabled: true,
        });
    }
    if folders {
        text(
            frame,
            Rect::new(inner.x, list.bottom(), inner.width, 4),
            format!(
                "{}\n{}",
                app.text("Destination", "Куда"),
                clean(&editor.move_folder)
            ),
            palette,
        );
    }
    if let Some(error) = &editor.error {
        text(
            frame,
            Rect::new(inner.x, footer.y - 3, inner.width, 3),
            error.clone(),
            Palette {
                text: palette.error,
                ..palette
            },
        );
    }
    let mut controls = vec![action(
        if folders {
            app.text("Move file", "Перенести файл")
        } else {
            app.text("Choose [Enter]", "Выбрать [Enter]")
        },
        if folders {
            TagAction::MoveConfirm
        } else {
            TagAction::ChooseDestination
        },
        if folders {
            !editor.move_folder.is_empty()
        } else {
            length > 0
        },
    )];
    if folders {
        controls.push(action(
            app.text("Enter folder [F]", "Указать папку [F]"),
            TagAction::FileInput(FileInput::Folder),
            true,
        ));
    }
    controls.push(action(app.text("Back", "Назад"), TagAction::Back, true));
    buttons(frame, app, footer, controls, palette);
}
