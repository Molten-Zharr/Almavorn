use super::super::{
    theme::Palette,
    widgets::{button, buttons, modal},
};
use crate::{
    app::{App, Target, TextDialog, TextPurpose},
    model::Language,
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn text_dialog(
    frame: &mut Frame,
    app: &mut App,
    dialog: &TextDialog,
    area: Rect,
    palette: Palette,
) {
    if dialog.color_picker.is_some() {
        super::color_picker::render(frame, app, dialog, area, palette);
        return;
    }
    let title = match &dialog.purpose {
        TextPurpose::Create | TextPurpose::CreateFromFolder { .. } => {
            app.text("Create playlist", "Создать плейлист")
        }
        TextPurpose::ComposePlaylist(ids) => {
            if ids.len() == 1 {
                app.text("Copy playlist", "Копия плейлиста")
            } else {
                app.text("Compose playlists", "Собрать плейлист")
            }
        }
        TextPurpose::RenamePlaylist(_) => app.text("Rename playlist", "Переименовать плейлист"),
        TextPurpose::PlaylistFolder(_, _) => app.text(
            "Playlist folder · full path",
            "Папка плейлиста · полный путь",
        ),
        TextPurpose::RemovePlaylistFolder(_, _) => app.text(
            "Unlink folder · tracks are kept",
            "Отключить папку · композиции сохраняются",
        ),
        TextPurpose::RenameTrack(_, _) => app.text(
            "Local track name · source tags untouched",
            "Имя в базе · исходные теги сохраняются",
        ),
        TextPurpose::Search => app.text(
            "Search · title, artist, album, path",
            "Поиск · название, исполнитель, альбом, путь",
        ),
        TextPurpose::Accent => app.text("Accent color · RRGGBB", "Цвет акцента · RRGGBB"),
        TextPurpose::Settings(edit) => edit.title(app.settings.language),
    };
    let inner = modal(frame, app, area, title.into(), 27, palette);
    let hint = if let TextPurpose::RemovePlaylistFolder(_, name) = &dialog.purpose {
        format!(
            "{}: {name}",
            app.text("Enter exact name or path", "Введите точное имя или путь")
        )
    } else if let TextPurpose::CreateFromFolder {
        folder,
        group_subfolders,
    } = &dialog.purpose
    {
        format!(
            "{}: {}\n{}",
            app.text("Folder", "Папка"),
            folder.display(),
            if *group_subfolders {
                app.text(
                    "Include subfolders as groups. Enter: create · Esc: cancel",
                    "Подпапки станут группами. Enter: создать · Esc: отмена",
                )
            } else {
                app.text(
                    "Only files in this folder. Enter: create · Esc: cancel",
                    "Только файлы этой папки. Enter: создать · Esc: отмена",
                )
            }
        )
    } else {
        app.text(
            "Enter: save · Esc: cancel",
            "Enter: сохранить · Esc: отмена",
        )
        .into()
    };
    let hint_height = (Paragraph::new(hint.clone())
        .wrap(Wrap { trim: false })
        .line_count(inner.width.max(1)) as u16
        + 1)
    .min(10);
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, hint_height),
    );
    let field = Rect::new(inner.x, inner.y + hint_height, inner.width, 3);
    frame.render_widget(
        Paragraph::new(format!("{}|", dialog.text))
            .scroll((
                0,
                dialog
                    .text
                    .chars()
                    .count()
                    .saturating_sub(inner.width.saturating_sub(4) as usize) as u16,
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(
                palette
                    .text()
                    .fg(palette.accent)
                    .bg(if dialog.selected_all {
                        palette.selection
                    } else {
                        palette.panel
                    }),
            ),
        field,
    );
    let mut keyboard_top = field.bottom() + 1;
    if matches!(
        dialog.purpose,
        TextPurpose::Create | TextPurpose::CreateFromFolder { .. }
    ) {
        let mut controls = vec![(
            app.text("From music folder", "Из папки с музыкой").into(),
            Target::PlaylistAutoName,
            !app.busy(),
        )];
        if let TextPurpose::CreateFromFolder {
            group_subfolders, ..
        } = &dialog.purpose
        {
            controls.push((
                format!(
                    "[{}] {}",
                    if *group_subfolders { 'x' } else { ' ' },
                    app.text("Group subfolders", "Подпапки в группы")
                ),
                Target::CreateFolderGroups,
                !app.busy(),
            ));
        }
        let used = buttons(
            frame,
            app,
            Rect::new(inner.x, keyboard_top, inner.width, 2),
            controls,
            palette,
        );
        keyboard_top += used + 1;
    }
    keyboard(
        frame,
        app,
        Rect::new(
            inner.x,
            keyboard_top,
            inner.width,
            inner
                .bottom()
                .saturating_sub(4)
                .saturating_sub(keyboard_top),
        ),
        dialog.keyboard,
        dialog.upper,
        palette,
    );
    let can_submit = match &dialog.purpose {
        TextPurpose::RemovePlaylistFolder(_, name) => &dialog.text == name,
        _ => true,
    };
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            (
                app.text("Confirm", "Подтвердить").into(),
                Target::Submit,
                can_submit,
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

pub(crate) fn keyboard(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    language: Language,
    upper: bool,
    palette: Palette,
) {
    let keyboard = if language == Language::Russian {
        vec![
            "йцукенгшщзхъ",
            "фывапролджэ",
            "ячсмитьбю",
            "1234567890",
            "ё _-()./:\\#",
        ]
    } else {
        vec![
            "qwertyuiop",
            "asdfghjkl",
            "zxcvbnm",
            "1234567890",
            " _-()./:\\#",
        ]
    };
    for (row, letters) in keyboard.iter().enumerate() {
        let y = area.y + row as u16;
        if y >= area.bottom().saturating_sub(2) {
            break;
        }
        for (column, c) in letters.chars().enumerate() {
            let c = if upper {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            };
            let x = area.x + column as u16 * 3;
            if x + 3 <= area.right() {
                button(
                    frame,
                    app,
                    Rect::new(x, y, 3, 1),
                    &c.to_string(),
                    Target::Text(c),
                    true,
                    palette,
                );
            }
        }
    }
    buttons(
        frame,
        app,
        Rect::new(area.x, area.bottom().saturating_sub(2), area.width, 2),
        vec![
            ("RU / EN".into(), Target::KeyboardLanguage, true),
            ("Shift".into(), Target::KeyboardCase, true),
            ("Backspace".into(), Target::Backspace, true),
            (app.text("Space", "Пробел").into(), Target::Text(' '), true),
        ],
        palette,
    );
}
