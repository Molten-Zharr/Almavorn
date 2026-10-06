mod buttons;
mod dialogs;
mod keycaps;
mod library;
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
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let palette = Palette::new(app);
    app.view.hits.clear();
    app.view.keycaps.clear();
    app.view.waveform_area = Rect::default();
    app.view.progress_area = Rect::default();
    app.view.filter_field = Rect::default();
    app.view.filter_keyboard_area = Rect::default();
    app.view.search_area = Rect::default();
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
    let footer_height = if area.width < 80 { 4 } else { 3 };
    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(9),
        Constraint::Length(footer_height),
    ])
    .split(area);
    let panel_label = app.text("Blocks", "Блоки");
    let panel_button_width =
        widgets::button_width(app, panel_label, &Target::Action(Action::Panels))
            .min(parts[0].width);
    let header = Line::from(vec![
        Span::styled(
            " ALMAVORN ",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}", app.text("Music player", "Музыкальный плеер")),
            Style::default().fg(palette.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(
            parts[0].x,
            parts[0].y,
            parts[0].width.saturating_sub(panel_button_width),
            1,
        ),
    );
    widgets::button(
        frame,
        app,
        Rect::new(
            parts[0].right().saturating_sub(panel_button_width),
            parts[0].y,
            panel_button_width,
            1,
        ),
        panel_label,
        Target::Action(Action::Panels),
        true,
        palette,
    );
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
    frame.render_widget(
        Paragraph::new(notice)
            .style(Style::default().fg(if app.view.notice_error {
                palette.error
            } else {
                palette.muted
            }))
            .wrap(Wrap { trim: false }),
        Rect::new(parts[2].x, parts[2].y, parts[2].width, 2),
    );
    let hint = if matches!(app.view.workspace.focus, crate::workspace::Panel::Player) {
        app.text(
            "↑↓ volume 1% · ←→ seek · Tab panel · Space pause · F1 help · Q quit",
            "↑↓ громкость 1% · ←→ перемотка · Tab панель · Space пауза · F1 помощь · Q выход",
        )
    } else {
        app.text(
            "↑↓ navigate · Tab panel · Enter play · Space pause · F1 help · Q quit",
            "↑↓ выбор · Tab панель · Enter играть · Space пауза · F1 помощь · Q выход",
        )
    };
    frame.render_widget(
        Paragraph::new(hint)
            .style(Style::default().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(
            parts[2].x,
            parts[2].y + 2,
            parts[2].width,
            footer_height - 2,
        ),
    );
    if app.view.dialog.is_none() && app.view.filter_keyboard {
        library::filter_keyboard(frame, app, palette);
    }
    if let Some(mut dialog) = app.view.dialog.take() {
        app.view.hits.clear();
        app.view.keycaps.clear();
        render_dialog(frame, app, &mut dialog, palette);
        app.view.dialog = Some(dialog);
    }
}
