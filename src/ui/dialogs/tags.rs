use super::metadata_tag_name;
use crate::{
    app::{App, Hit, TagAction, TagEdit, TagEditor, TagsTab, Target},
    tags::{field_keys, type_name},
    ui::{
        theme::Palette,
        widgets::{block, button, buttons, clean, file_modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
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
    let inner = file_modal(
        frame,
        app,
        area,
        app.text("File tag editor", "Редактор тегов файла").into(),
        42,
        palette,
    );
    if inner.width < 24 || inner.height < 16 {
        text(
            frame,
            inner,
            app.text(
                "Enlarge the window. Esc: back",
                "Увеличьте окно. Esc: назад",
            ),
            palette,
        );
        return;
    }
    let bottom = inner.bottom();
    let footer = Rect::new(inner.x, bottom - 3, inner.width, 3);
    if app.writing_tags() {
        text(
            frame,
            inner,
            app.text(
                "Writing tags and updating the library…",
                "Запись тегов и обновление библиотеки…",
            ),
            palette,
        );
        return;
    }
    if editor.discard {
        text(
            frame,
            inner,
            format!(
                "{}\n\n{}",
                editor.filename(),
                app.text(
                    "Discard unsaved changes? The audio file has not been changed.",
                    "Отменить несохраненные изменения? Аудиофайл еще не изменен."
                )
            ),
            palette,
        );
        buttons(
            frame,
            app,
            footer,
            vec![
                action(
                    app.text("Discard", "Отменить изменения"),
                    TagAction::Discard,
                    true,
                ),
                action(app.text("Back", "Вернуться"), TagAction::Back, true),
            ],
            palette,
        );
        return;
    }
    if editor.confirm {
        let explanation = format!("{}\n{}\n\n{}\n{}\n\n{}", clean(&editor.filename()), clean(&editor.track.path.display().to_string()),
            app.text("Tags will be written to the audio file.", "Теги будут записаны в аудиофайл."),
            app.text("All playlists using this file will show the new metadata. Chaos undo cannot undo file writes.",
                "Новые сведения появятся во всех плейлистах с этим файлом. Отмена Хаоса не отменяет запись файлов."),
            if editor.destructive() { app.text("Clear the selected tags and write the changes?", "Очистить отмеченные теги и записать изменения?") }
            else { app.text("Save these changes?", "Сохранить изменения?") }
        );
        text(
            frame,
            Rect::new(inner.x, inner.y, inner.width, inner.height - 7),
            explanation,
            palette,
        );
        if let Some(error) = &editor.error {
            text(
                frame,
                Rect::new(inner.x, bottom - 7, inner.width, 4),
                error.clone(),
                Palette {
                    text: palette.error,
                    ..palette
                },
            );
        }
        buttons(
            frame,
            app,
            footer,
            vec![
                action(
                    app.text("Write file", "Записать файл"),
                    TagAction::Confirm,
                    true,
                ),
                action(app.text("Cancel", "Отмена"), TagAction::Back, true),
            ],
            palette,
        );
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
    let current = &document.blocks[editor.block];
    text(
        frame,
        Rect::new(inner.x, inner.y, inner.width, 1),
        format!(
            "{}  ·  {}: {}",
            clean(&editor.filename()),
            app.text("Playlists", "Плейлистов"),
            editor.shared
        ),
        palette,
    );
    let header_height = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y + 1, inner.width, 3),
        vec![
            action("◀", TagAction::Format(-1), editor.block > 0),
            (
                format!(
                    "{}{}",
                    type_name(current.kind),
                    if current.existed { "" } else { " +" }
                ),
                Target::Tags(TagAction::Format(1)),
                editor.block + 1 < document.blocks.len(),
            ),
            action(
                "▶",
                TagAction::Format(1),
                editor.block + 1 < document.blocks.len(),
            ),
            action(
                app.text("Fields", "Поля"),
                TagAction::Tab(TagsTab::Fields),
                true,
            ),
            action(
                app.text("Extra", "Дополнительно"),
                TagAction::Tab(TagsTab::Extras),
                true,
            ),
            action(
                app.text("Covers", "Обложки"),
                TagAction::Tab(TagsTab::Covers),
                true,
            ),
            action(
                app.text("Clear", "Очистка"),
                TagAction::Tab(TagsTab::Cleanup),
                true,
            ),
        ],
        palette,
    )
    .min(3);
    let y = inner.y + 1 + header_height;
    let hint = if editor.key_picker {
        app.text(
            "Choose a field. Enter or double-click: add.",
            "Выберите поле. Enter или двойной клик: добавить.",
        )
    } else {
        match editor.tab {
            TagsTab::Fields | TagsTab::Extras => app.text(
                "Checkbox: clear field. Enter or double-click: edit.",
                "Галочка: очистить поле. Enter или двойной клик: изменить.",
            ),
            TagsTab::Covers => app.text(
                "Checkbox: remove cover. Enter or double-click: edit.",
                "Галочка: удалить обложку. Enter или двойной клик: изменить.",
            ),
            TagsTab::Cleanup => app.text(
                "Checked blocks will be removed from the file on Save.",
                "Отмеченные блоки будут удалены из файла после сохранения.",
            ),
        }
    };
    text(frame, Rect::new(inner.x, y, inner.width, 2), hint, palette);
    let error_height = if editor.error.is_some() { 3 } else { 0 };
    let list = Rect::new(
        inner.x,
        y + 2,
        inner.width,
        (bottom - 8 - error_height).saturating_sub(y + 2),
    );
    let keys = field_keys(current.kind);
    let length = editor.length();
    editor.selected = editor.selected.min(length.saturating_sub(1));
    editor.offset = visible_offset(editor.offset, editor.selected, list.height as usize, length);
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
        .take(list.height as usize)
        .enumerate()
    {
        let row = Rect::new(list.x, list.y + line as u16, list.width, 1);
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
        frame.render_widget(
            Paragraph::new(if editor.key_picker {
                value
            } else {
                checkbox(removed, &value)
            })
            .style(style),
            row,
        );
        app.view.hits.push(Hit {
            area: row,
            target: Target::Tags(TagAction::Row(index)),
            enabled: true,
        });
        if !editor.key_picker {
            app.view.hits.push(Hit {
                area: Rect::new(row.x, row.y, row.width.min(3), 1),
                target: Target::Tags(if editor.tab == TagsTab::Cleanup {
                    TagAction::ClearBlock(index)
                } else {
                    TagAction::Remove(index)
                }),
                enabled: true,
            });
        }
    }
    let controls = Rect::new(inner.x, bottom - 8 - error_height, inner.width, 5);
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
        if current.has_special_fields() {
            text(
                frame,
                Rect::new(controls.x, controls.y + 2, controls.width, 3),
                app.text(
                    "Additional data is on the Extra tab.",
                    "Дополнительные данные доступны на вкладке Дополнительно.",
                ),
                Palette {
                    text: palette.muted,
                    ..palette
                },
            );
        }
    }
    if let Some(error) = &editor.error {
        text(
            frame,
            Rect::new(
                inner.x,
                bottom - 3 - error_height,
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
    buttons(
        frame,
        app,
        footer,
        vec![
            action(
                app.text("Save file", "Сохранить файл"),
                TagAction::Save,
                document.changed() && !editor.key_picker,
            ),
            action(app.text("Close", "Закрыть"), TagAction::Back, true),
        ],
        palette,
    );
}

fn render_edit(
    frame: &mut Frame,
    app: &mut App,
    edit: &mut TagEdit,
    error: Option<&str>,
    inner: Rect,
    palette: Palette,
) {
    let title = if edit.extra {
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
    let hint =
        if edit.cover {
            app.text("Image path: leave empty to keep it. Type: 3 front, 4 back, 0–20 ID3 types.",
            "Путь к картинке: пустой сохраняет прежнюю. Тип: 3 лицевая, 4 оборот, 0–20 типы ID3.")
        } else {
            app.text(
                "Tab: next input. Ctrl+A: select all. F10: newline. Enter: apply to draft.",
                "Tab: следующее поле. Ctrl+A: выделить все. F10: новая строка. Enter: в черновик.",
            )
        };
    text(
        frame,
        Rect::new(inner.x, inner.y + 1, inner.width, 3),
        hint,
        palette,
    );
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
    let value_height = inner.height.saturating_sub(16).max(3);
    let mut y = inner.y + 4;
    for (index, title) in titles
        .iter()
        .take(if edit.extra { 2 } else { 3 })
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
        let remaining = inner.bottom().saturating_sub(y + 3);
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
    } else if !edit.cover && !edit.extra && y < inner.bottom() - 3 {
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
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom() - 3, inner.width, 3),
        vec![
            action(app.text("Apply", "Применить"), TagAction::Apply, true),
            action(app.text("Back", "Назад"), TagAction::Back, true),
        ],
        palette,
    );
}
