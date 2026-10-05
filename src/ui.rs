mod dialogs;
mod library;
mod player;
mod settings;
mod theme;
mod toolbar;
mod widgets;

use self::{
    dialogs::render_dialog,
    library::{playlists, tracks_table},
    player::{player, preferred_height},
    theme::Palette,
};
use crate::{app::App, model::Placement};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let palette = Palette::new(app);
    app.hits.clear();
    if app
        .dialog
        .as_ref()
        .is_some_and(crate::app::Dialog::is_settings)
    {
        settings::render_settings(frame, app, palette);
        if !matches!(app.dialog, Some(crate::app::Dialog::Settings { .. }))
            && let Some(mut dialog) = app.dialog.take()
        {
            app.hits.clear();
            render_dialog(frame, app, &mut dialog, palette);
            app.dialog = Some(dialog);
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
    let groups = toolbar::commands(app);
    let header_height = 1;
    let footer_height = if area.width < 80 { 4 } else { 3 };
    let player_height = if area.width >= 80
        && matches!(
            app.settings.player_placement,
            Placement::Left | Placement::Right
        ) {
        0
    } else {
        preferred_height(app, area.width)
    };
    let library_width = if player_height == 0 {
        area.width.saturating_sub(30)
    } else {
        area.width
    };
    let library_height = if library_width >= 70
        && matches!(
            app.settings.playlist_placement,
            Placement::Left | Placement::Right
        ) {
        9
    } else {
        11
    };
    let reserved = header_height + footer_height + player_height + library_height;
    let compact =
        toolbar::height(app, &groups, area.width, false).saturating_add(reserved) > area.height;
    let toolbar_height = toolbar::height(app, &groups, area.width, compact);
    if toolbar_height.saturating_add(reserved) > area.height {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window or reduce zoom to fit all controls.",
                "Увеличьте окно или уменьшите масштаб, чтобы все элементы поместились.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            area,
        );
        if let Some(mut dialog) = app.dialog.take() {
            render_dialog(frame, app, &mut dialog, palette);
            app.dialog = Some(dialog);
        }
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(toolbar_height),
        Constraint::Min(9),
        Constraint::Length(footer_height),
    ])
    .split(area);
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
        Rect::new(parts[0].x, parts[0].y, parts[0].width, 1),
    );
    toolbar::render(frame, app, parts[1], groups, compact, palette);
    let mut player_placement = app.settings.player_placement;
    if area.width < 80 && matches!(player_placement, Placement::Left | Placement::Right) {
        player_placement = Placement::Bottom;
    }
    let vertical = matches!(player_placement, Placement::Left | Placement::Right);
    let player_first = matches!(player_placement, Placement::Top | Placement::Left);
    let size = if vertical {
        30
    } else {
        preferred_height(app, area.width)
    };
    let content = Layout::default()
        .direction(if vertical {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints(if player_first {
            vec![Constraint::Length(size), Constraint::Min(5)]
        } else {
            vec![Constraint::Min(5), Constraint::Length(size)]
        })
        .split(parts[2]);
    let (player_area, library_area) = if player_first {
        (content[0], content[1])
    } else {
        (content[1], content[0])
    };
    player(frame, app, player_area, palette);
    let mut playlist_placement = app.settings.playlist_placement;
    if library_area.width < 70 && matches!(playlist_placement, Placement::Left | Placement::Right) {
        playlist_placement = Placement::Top;
    }
    let horizontal = matches!(playlist_placement, Placement::Left | Placement::Right);
    let list_first = matches!(playlist_placement, Placement::Left | Placement::Top);
    let size = if horizontal {
        (library_area.width / 4).clamp(20, 34)
    } else {
        7
    };
    let panels = Layout::default()
        .direction(if horizontal {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints(if list_first {
            vec![Constraint::Length(size), Constraint::Min(4)]
        } else {
            vec![Constraint::Min(4), Constraint::Length(size)]
        })
        .split(library_area);
    let (list, tracks) = if list_first {
        (panels[0], panels[1])
    } else {
        (panels[1], panels[0])
    };
    playlists(frame, app, list, palette);
    tracks_table(frame, app, tracks, palette);
    let notice = if app.notice_error {
        app.notice.clone()
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
        app.notice.clone()
    };
    frame.render_widget(
        Paragraph::new(notice)
            .style(Style::default().fg(if app.notice_error {
                palette.error
            } else {
                palette.muted
            }))
            .wrap(Wrap { trim: false }),
        Rect::new(parts[3].x, parts[3].y, parts[3].width, 2),
    );
    frame.render_widget(
        Paragraph::new(app.text(
            "↑↓ navigate · Tab panel · Enter play · Space pause · F1 help · Q quit",
            "↑↓ выбор · Tab панель · Enter играть · Space пауза · F1 помощь · Q выход",
        ))
        .style(Style::default().fg(palette.muted))
        .wrap(Wrap { trim: false }),
        Rect::new(
            parts[3].x,
            parts[3].y + 2,
            parts[3].width,
            footer_height - 2,
        ),
    );
    if let Some(mut dialog) = app.dialog.take() {
        app.hits.clear();
        render_dialog(frame, app, &mut dialog, palette);
        app.dialog = Some(dialog);
    }
}
