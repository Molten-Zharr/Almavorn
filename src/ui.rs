mod buttons;
mod dialogs;
mod keycaps;
mod library;
mod menus;
mod player;
mod settings;
mod theme;
mod toolbar;
mod widgets;
mod workspace;

pub(crate) fn workspace_fits(app: &App, root: &crate::workspace::Dock) -> bool {
    workspace::fits(app, root)
}

use self::{dialogs::render_dialog, theme::Palette};
use crate::{
    app::{App, Target},
    input::Action,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let palette = Palette::new(app);
    app.view.hits.clear();
    app.view.keycaps.clear();
    app.view.dialog_scroll_max = 0;
    app.view.waveform_area = Rect::default();
    app.view.progress_area = Rect::default();
    app.view.filter_field = Rect::default();
    app.view.filter_keyboard_area = Rect::default();
    app.view.overlay_area = Rect::default();
    if app
        .view
        .dialog
        .as_ref()
        .is_some_and(crate::app::Dialog::is_settings)
    {
        settings::render_settings(frame, app, palette);
        if !matches!(app.view.dialog, Some(crate::app::Dialog::Settings { .. }))
            && let Some(mut dialog) = app.view.dialog.take()
        {
            app.view.hits.clear();
            app.view.keycaps.clear();
            render_dialog(frame, app, &mut dialog, palette);
            app.view.dialog = Some(dialog);
        }
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(palette.background)),
        area,
    );
    if area.width < 32 || area.height < 18 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window (at least 32 × 18 cells).",
                "Увеличьте окно (не менее 32 × 18 ячеек).",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(toolbar::height(app, area.width)),
        Constraint::Min(9),
        Constraint::Length(1),
    ])
    .split(area);
    toolbar::render(frame, app, parts[0], palette);
    workspace::render(frame, app, parts[1], palette);
    let notice = if app.view.notice_error {
        app.view.notice.clone()
    } else if app.importing() {
        format!(
            "{}: {}",
            app.text("Reading files", "Чтение файлов"),
            app.progress()
        )
    } else if app.busy() {
        app.text("Updating library…", "Обновление библиотеки…")
            .to_owned()
    } else {
        app.view.notice.clone()
    };
    let tooltip = app.hovered_target().and_then(|target| match target {
        Target::Action(action) => Some(format!(
            "{}{}",
            buttons::button_shortcut(app, target).map_or(String::new(), |key| format!("{key} · ")),
            action.description(app.settings.language)
        )),
        Target::CommandMenu(menu) => Some(menu.title(app.settings.language).to_owned()),
        _ => None,
    });
    let message = if app.view.notice_error || app.busy() || app.importing() {
        notice
    } else if app.view.layout_editing {
        app.text(
            "Drag a title or border · Ctrl+B / Esc: done",
            "Тяните заголовок или границу · Ctrl+B / Esc: готово",
        )
        .into()
    } else if app.view.toolbar_selected.is_some() {
        app.text(
            "←→: select · Enter: open · Esc: library",
            "←→: выбор · Enter: открыть · Esc: библиотека",
        )
        .into()
    } else {
        tooltip.unwrap_or(notice)
    };
    let help_target = Target::Action(Action::Help);
    let help = format!(
        "{} {}",
        buttons::button_shortcut(app, &help_target).unwrap_or_default(),
        app.text("Help", "Справка")
    );
    let help_width = (Span::raw(&help).width() as u16 + 2).min(parts[2].width);
    let navigation = if area.width >= 80 {
        app.text("Tab: panels / menu", "Tab: панели / меню")
    } else {
        ""
    };
    let navigation_width = Span::raw(navigation).width() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(message)).style(
            Style::default()
                .fg(if app.view.notice_error {
                    palette.error
                } else {
                    palette.muted
                })
                .bg(palette.background),
        ),
        Rect::new(
            parts[2].x,
            parts[2].y,
            parts[2]
                .width
                .saturating_sub(help_width + navigation_width + 2),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(navigation).style(palette.text().fg(palette.muted)),
        Rect::new(
            parts[2]
                .right()
                .saturating_sub(help_width + navigation_width + 1),
            parts[2].y,
            navigation_width,
            1,
        ),
    );
    buttons::quiet_button(
        frame,
        app,
        Rect::new(
            parts[2].right().saturating_sub(help_width),
            parts[2].y,
            help_width,
            1,
        ),
        &help,
        help_target,
        true,
        palette,
    );
    if app.view.dialog.is_none() && app.view.filter_keyboard {
        library::filter_keyboard(frame, app, palette);
    }
    if let Some(mut dialog) = app.view.dialog.take() {
        app.view.hits.clear();
        app.view.keycaps.clear();
        if let crate::app::Dialog::Commands {
            menu,
            selected,
            anchor,
        } = &mut dialog
        {
            menus::render(frame, app, *menu, selected, *anchor, palette);
        } else {
            render_dialog(frame, app, &mut dialog, palette);
        }
        app.view.dialog = Some(dialog);
    }
}
