use super::super::{
    theme::Palette,
    widgets::{buttons, clean, file_modal, modal, visible_offset},
};
use crate::app::{App, Browser, Hit, PaletteFilePicker, Target};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn browser_dialog(
    frame: &mut Frame,
    app: &mut App,
    browser: &mut Browser,
    area: Rect,
    palette: Palette,
) {
    let title = match &browser.palette_file {
        Some(PaletteFilePicker::Import) => {
            app.text("Import palette · JSON", "Импорт палитры · JSON")
        }
        Some(PaletteFilePicker::Export { .. }) => app.text(
            "Export palette · choose folder",
            "Экспорт палитры · выбрать папку",
        ),
        None if browser.playlist_name.is_some() => {
            app.text("Playlist from folder", "Плейлист из папки")
        }
        None if browser.folder_playlists.is_some() => {
            app.text("Folders to playlists", "Папки в плейлисты")
        }
        None if browser.folder => app.text("Add folder", "Добавить папку"),
        None => app.text("Add files", "Добавить файлы"),
    };
    let inner = if browser.palette_file.is_some() {
        file_modal(frame, app, area, title.into(), 42, palette)
    } else {
        modal(frame, app, area, title.into(), 42, palette)
    };
    let path_height = if browser.palette_file.is_some() && inner.height < 12 {
        1
    } else {
        2
    };
    frame.render_widget(
        Paragraph::new(browser.directory.display().to_string())
            .style(palette.text().fg(palette.folder_path))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, path_height),
    );
    let mut controls = vec![
        (
            app.text("Parent", "Выше").into(),
            Target::BrowserParent,
            browser.directory.parent().is_some(),
        ),
        (
            if browser.palette_file.is_some() || browser.folder_playlists.is_some() {
                app.text("Open", "Открыть")
            } else {
                app.text("Select / open", "Выбрать / открыть")
            }
            .into(),
            Target::BrowserOpen,
            !browser.entries.is_empty(),
        ),
    ];
    if browser.palette_file.is_some() {
        controls.extend([
            (
                app.text("Home", "Домой").into(),
                Target::BrowserHome,
                dirs::home_dir().is_some(),
            ),
            (
                app.text("Documents", "Документы").into(),
                Target::BrowserDocuments,
                dirs::document_dir().is_some(),
            ),
        ]);
    }
    if let Some(group_subfolders) = browser.folder_playlists {
        controls.extend([
            (
                app.text("Mark all", "Отметить все").into(),
                Target::BrowserMarkAll,
                app.browser_ready() && !browser.entries.is_empty(),
            ),
            (
                app.text("Clear marks", "Снять отметки").into(),
                Target::BrowserClearMarks,
                !browser.marked.is_empty(),
            ),
            (
                format!(
                    "[{}] {}",
                    if group_subfolders { 'x' } else { ' ' },
                    app.text("Subfolders as groups", "Подпапки в группы")
                ),
                Target::CreateFolderGroups,
                !app.busy(),
            ),
        ]);
    } else if browser.playlist_name.is_none() && browser.palette_file.is_none() {
        controls.push((
            if browser.folder {
                format!(
                    "[{}] {}",
                    if app.settings.scan_subfolders {
                        'x'
                    } else {
                        ' '
                    },
                    app.text("Search subfolders", "Искать в подпапках")
                )
            } else {
                app.text("Mark all", "Отметить все").into()
            },
            if browser.folder {
                Target::ScanSubfolders
            } else {
                Target::BrowserMarkAll
            },
            if browser.folder {
                !app.busy()
            } else {
                app.browser_ready() && !browser.entries.is_empty()
            },
        ));
    }
    let used = buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.y + path_height,
            inner.width,
            if browser.folder_playlists.is_some() {
                4
            } else {
                3
            },
        ),
        controls,
        palette,
    );
    let music_folder =
        browser.folder && browser.folder_playlists.is_none() && browser.palette_file.is_none();
    let current_choice = music_folder && browser.selection_explicit;
    let footer_height = if music_folder || browser.folder_playlists.is_some() {
        3
    } else {
        2
    };
    let list_top = inner.y + path_height + used;
    let list = Rect::new(
        inner.x,
        list_top,
        inner.width,
        inner
            .bottom()
            .saturating_sub(footer_height)
            .saturating_sub(list_top),
    );
    if app.browser_loading() {
        frame.render_widget(
            Paragraph::new(app.text("Reading folder...", "Чтение папки...")).style(palette.text()),
            list,
        );
    }
    if app.browser_ready()
        && browser.entries.is_empty()
        && (browser.palette_file.is_some() || browser.folder_playlists.is_some())
    {
        frame.render_widget(
            Paragraph::new(if browser.folder_playlists.is_some() {
                app.text(
                    "No folders here. Go to the parent folder.",
                    "Здесь нет папок. Перейдите выше.",
                )
            } else if browser.folder {
                app.text(
                    "No subfolders. You can choose the current folder.",
                    "Нет подпапок. Можно выбрать текущую папку.",
                )
            } else {
                app.text(
                    "No JSON files in this folder.",
                    "В этой папке нет JSON-файлов.",
                )
            })
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
            list,
        );
    }
    browser.offset = visible_offset(
        browser.offset,
        browser.selected,
        list.height as usize,
        browser.entries.len(),
    );
    for (index, entry) in browser
        .entries
        .iter()
        .enumerate()
        .skip(browser.offset)
        .take(list.height as usize)
    {
        let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
        let marker = if browser.folder_playlists.is_some() {
            if browser.marked.contains(&entry.path) {
                "[x]"
            } else {
                "[ ]"
            }
        } else if entry.directory {
            "DIR"
        } else if browser.palette_file.is_some() {
            "JSON"
        } else if browser.marked.contains(&entry.path) {
            "[x]"
        } else {
            "[ ]"
        };
        let rect = Rect::new(
            list.x,
            list.y + (index - browser.offset) as u16,
            list.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{marker} {}{}",
                clean(&name),
                if entry.directory { "/" } else { "" }
            ))
            .style(if browser.selected == index {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else if entry.directory {
                palette.text().fg(palette.folder_icon)
            } else {
                palette.text()
            }),
            rect,
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::BrowserRow(index),
            enabled: true,
        });
        if browser.folder_playlists.is_some() {
            app.view.hits.push(Hit {
                area: Rect::new(rect.x, rect.y, rect.width.min(3), 1),
                target: Target::BrowserMark(index),
                enabled: app.browser_ready(),
            });
        }
    }
    let add = if let Some(purpose) = &browser.palette_file {
        match purpose {
            PaletteFilePicker::Import => app
                .text(
                    "Import",
                    if inner.width < 40 {
                        "Импорт"
                    } else {
                        "Импортировать"
                    },
                )
                .into(),
            PaletteFilePicker::Export { .. } if inner.width < 40 => {
                app.text("Choose", "Выбрать").into()
            }
            PaletteFilePicker::Export { .. } if browser.selection_explicit => {
                app.text("Selected folder", "Выбранную папку").into()
            }
            PaletteFilePicker::Export { .. } => app.text("Current folder", "Текущую папку").into(),
        }
    } else if browser.folder_playlists.is_some() {
        format!(
            "{} ({})",
            app.text("Create", "Создать"),
            browser.marked.len()
        )
    } else if browser.playlist_name.is_some() {
        if browser.selection_explicit {
            app.text("Selected folder", "Выбранную папку").into()
        } else {
            app.text("Current folder", "Текущую папку").into()
        }
    } else if browser.folder {
        app.text("Add folder", "Добавить папку").into()
    } else {
        format!(
            "{} ({})",
            app.text("Add selected", "Добавить выбранные"),
            browser.marked.len()
        )
    };
    let mut footer = vec![(
        add,
        Target::BrowserAdd,
        app.browser_ready()
            && if browser.folder_playlists.is_some() {
                !browser.marked.is_empty()
            } else {
                browser.folder
                    || !browser.marked.is_empty()
                    || browser
                        .entries
                        .get(browser.selected)
                        .is_some_and(|entry| !entry.directory)
            },
    )];
    if current_choice {
        footer.push((
            app.text("Current folder", "Текущую папку").into(),
            Target::BrowserCurrentFolder,
            app.browser_ready(),
        ));
    }
    footer.push((
        app.text("Cancel", "Отмена").into(),
        Target::CloseDialog,
        true,
    ));
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer_height),
            inner.width,
            footer_height,
        ),
        footer,
        palette,
    );
}
