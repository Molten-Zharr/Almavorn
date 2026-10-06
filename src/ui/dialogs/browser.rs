use super::super::{
    theme::Palette,
    widgets::{buttons, clean, modal, visible_offset},
};
use crate::app::{App, Browser, Hit, Target};
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
    let inner = modal(
        frame,
        area,
        app.text(
            if browser.folder {
                "Add folder"
            } else {
                "Add files"
            },
            if browser.folder {
                "Добавить папку"
            } else {
                "Добавить файлы"
            },
        )
        .into(),
        42,
        palette,
    );
    frame.render_widget(
        Paragraph::new(browser.directory.display().to_string())
            .style(palette.text().fg(palette.folder_path))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, 2),
    );
    let used = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y + 2, inner.width, 3),
        vec![
            (
                app.text("Parent", "Выше").into(),
                Target::BrowserParent,
                browser.directory.parent().is_some(),
            ),
            (
                app.text("Select / open", "Выбрать / открыть").into(),
                Target::BrowserOpen,
                !browser.entries.is_empty(),
            ),
            (
                app.text("Mark all", "Отметить все").into(),
                Target::BrowserMarkAll,
                !browser.folder && app.browser_ready() && !browser.entries.is_empty(),
            ),
        ],
        palette,
    );
    let list = Rect::new(
        inner.x,
        inner.y + 2 + used,
        inner.width,
        inner.height.saturating_sub(4 + used),
    );
    if app.browser_loading() {
        frame.render_widget(
            Paragraph::new(app.text("Reading folder...", "Чтение папки...")).style(palette.text()),
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
        let marker = if entry.directory {
            "DIR"
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
    }
    let add = if browser.folder {
        app.text("Add folder", "Добавить папку").into()
    } else {
        format!(
            "{} ({})",
            app.text("Add selected", "Добавить выбранные"),
            browser.marked.len()
        )
    };
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            (
                add,
                Target::BrowserAdd,
                app.browser_ready()
                    && (browser.folder
                        || !browser.marked.is_empty()
                        || browser
                            .entries
                            .get(browser.selected)
                            .is_some_and(|entry| !entry.directory)),
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
