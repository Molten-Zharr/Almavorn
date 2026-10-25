use crate::{
    app::{App, Hit, ScanFolderAction, ScanFolders, Target},
    ui::{
        theme::Palette,
        widgets::{buttons, buttons_height, clean, modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut ScanFolders,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Find new tracks", "Добавить новые композиции")
            .into(),
        38,
        palette,
    );
    let controls = vec![
        (
            app.text("Mark all", "Отметить все").into(),
            Target::ScanFolders(ScanFolderAction::All(true)),
            !view.rows.is_empty(),
        ),
        (
            app.text("Clear marks", "Снять отметки").into(),
            Target::ScanFolders(ScanFolderAction::All(false)),
            view.rows.iter().any(|r| r.checked),
        ),
        (
            format!(
                "[{}] {}",
                if app.settings.preferences.scan_subfolders {
                    'x'
                } else {
                    ' '
                },
                app.text("Playlist subfolders", "Подпапки плейлистов")
            ),
            Target::ScanSubfolders,
            !app.busy(),
        ),
    ];
    let used = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y, inner.width, inner.height.min(3)),
        controls,
        palette,
    );
    let hint = app.text(
        "New tracks go to the selected playlist or desk group. Desk scans only existing group folders. Space: mark.",
        "Новые треки попадут в выбранный плейлист или группу стола. На столе проверяются только папки существующих групп. Пробел: отметить.",
    );
    let hint_height = (Paragraph::new(hint)
        .wrap(Wrap { trim: false })
        .line_count(inner.width.max(1)) as u16)
        .min(inner.height.saturating_sub(used));
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y + used, inner.width, hint_height),
    );
    let count = view.rows.iter().filter(|r| r.checked).count();
    let footer = vec![
        (
            format!("{} ({count})", app.text("Find new", "Найти новые")),
            Target::ScanFolders(ScanFolderAction::Start),
            count > 0 && !app.busy(),
        ),
        (
            app.text("Cancel", "Отмена").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    let footer_height = buttons_height(app, inner.width, &footer)
        .min(inner.height.saturating_sub(used + hint_height));
    let list = Rect::new(
        inner.x,
        inner.y + used + hint_height,
        inner.width,
        inner
            .height
            .saturating_sub(used + hint_height + footer_height),
    );
    view.capacity = list.height as usize;
    view.offset = visible_offset(view.offset, view.selected, view.capacity, view.rows.len());
    for (index, row) in view
        .rows
        .iter()
        .enumerate()
        .skip(view.offset)
        .take(view.capacity)
    {
        let rect = Rect::new(list.x, list.y + (index - view.offset) as u16, list.width, 1);
        let label = format!(
            "[{}] {} · {}",
            if row.checked { 'x' } else { ' ' },
            clean(&row.name),
            clean(&row.folder.display().to_string())
        );
        frame.render_widget(
            Paragraph::new(label).style(if index == view.selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            }),
            rect,
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::ScanFolders(ScanFolderAction::Row(index)),
            enabled: true,
        });
    }
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
