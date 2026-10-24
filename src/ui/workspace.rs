use super::{
    buttons::{button_width, quiet_button},
    library, player,
    theme::Palette,
    widgets::block,
};
use crate::layout::{
    title_reserved,
    workspace::{drop_preview, resize_handle},
    workspace_metrics,
};
use crate::{
    app::{App, CommandMenu, Hit, Target},
    input::Action,
    workspace::{Axis, Panel},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

fn chrome(
    frame: &mut Frame,
    app: &mut App,
    panel: Panel,
    area: Rect,
    compact: bool,
    palette: Palette,
) -> Rect {
    let active = app.view.toolbar_selected.is_none() && app.view.workspace.focus == panel;
    let outer = block(String::new(), palette, false)
        .borders(
            if palette.borders == crate::preferences::BorderWeight::None {
                Borders::NONE
            } else if compact {
                Borders::TOP | Borders::LEFT | Borders::RIGHT
            } else {
                Borders::ALL
            },
        )
        .border_style(Style::default().fg(palette.separator));
    let inner = if palette.borders == crate::preferences::BorderWeight::None {
        Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        )
    } else {
        outer.inner(area)
    };
    frame.render_widget(outer, area);
    app.view.hits.push(Hit {
        area,
        target: Target::PanelFocus(panel),
        enabled: true,
    });
    let title_width = area.width.saturating_sub(title_reserved(app, panel));
    let heading = if panel == Panel::Tracks {
        app.playlist()
            .map(|playlist| {
                let name = playlist.display_name(app.settings.preferences.language);
                if playlist.kind == crate::model::PlaylistKind::SortingDesk {
                    app.text(
                        "Sorting desk · Organize music",
                        "Сортировочный стол · Разобрать музыку",
                    )
                    .to_owned()
                } else if playlist.mode == crate::model::Mode::Order {
                    format!(
                        "{} · {name}",
                        if app.playlist_unlocked(playlist.id) {
                            app.text("Editing", "Редактирование")
                        } else {
                            app.text("Protected", "Защищен")
                        }
                    )
                } else {
                    name.to_owned()
                }
            })
            .unwrap_or_else(|| panel.name(app.settings.preferences.language).into())
    } else if panel == Panel::Player {
        app.playback_source_caption()
    } else {
        panel.name(app.settings.preferences.language).into()
    };
    let title = heading
        .chars()
        .take(usize::from(title_width))
        .collect::<String>();
    let title_area = Rect::new(area.x + 1, area.y, title_width, 1);
    let title_hit = Rect::new(title_area.x, title_area.y, title.chars().count() as u16, 1);
    frame.render_widget(
        Paragraph::new(title).style(palette.text().fg(if active {
            palette.accent
        } else {
            palette.muted
        })),
        title_hit,
    );
    if app.view.layout_editing {
        app.view.hits.push(Hit {
            area: title_hit,
            target: Target::PanelMove(panel),
            enabled: true,
        });
    } else {
        if panel == Panel::Playlists
            && app.control_visible(Panel::PlaylistActions, Action::NewPlaylist)
        {
            let target = Target::Action(Action::NewPlaylist);
            let width = button_width(app, "+", &target);
            quiet_button(
                frame,
                app,
                Rect::new(area.right().saturating_sub(width + 5), area.y, width, 1)
                    .intersection(area),
                "+",
                target,
                app.allowed(Action::NewPlaylist),
                palette,
            );
        }
        if matches!(panel, Panel::Playlists | Panel::Tracks | Panel::Player) {
            quiet_button(
                frame,
                app,
                Rect::new(area.right().saturating_sub(4), area.y, 3, 1).intersection(area),
                "...",
                Target::CommandMenu(match panel {
                    Panel::Playlists => CommandMenu::Playlist,
                    Panel::Player => CommandMenu::Player,
                    _ => CommandMenu::Tracks,
                }),
                true,
                palette,
            );
        }
        return inner;
    }
    let collapsed = app
        .settings
        .preferences
        .workspace
        .collapsed
        .contains(&panel);
    for (index, (label, target)) in [
        ("[↕]", Target::PanelMove(panel)),
        (
            if collapsed { "[+]" } else { "[-]" },
            Target::PanelCollapse(panel),
        ),
        ("[x]", Target::PanelClose(panel)),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            area.right().saturating_sub(12) + index as u16 * 4,
            area.y,
            3,
            1,
        )
        .intersection(area);
        let style = palette.text().bg(if app.hovered(rect) {
            palette.selection
        } else {
            palette.background
        });
        let symbol_style = style.fg(if active || app.hovered(rect) {
            palette.accent
        } else {
            palette.muted
        });
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("[", style.fg(palette.accent)),
                Span::styled(label.trim_matches(['[', ']']), symbol_style),
                Span::styled("]", style.fg(palette.accent)),
            ]))
            .style(style),
            rect,
        );
        app.view.hits.push(Hit {
            area: rect,
            target,
            enabled: true,
        });
    }
    inner
}

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.view.workspace.bounds = area;
    app.view.workspace.areas.clear();
    app.view.workspace.splits.clear();
    app.view.playlist_area = Rect::default();
    app.view.tracks_area = Rect::default();
    let metrics = workspace_metrics(app);
    let root = app.settings.preferences.workspace.root.as_ref();
    let geometry = if app.settings.preferences.artwork.visible && app.playback.current.is_some() {
        metrics.arrange_with_artwork(root, area)
    } else {
        metrics.arrange(root, area)
    };
    let compact = geometry.compact;
    app.view.workspace.areas = geometry.areas;
    app.view.workspace.splits = geometry.splits;
    app.view.workspace.root = Some(geometry.root);
    if !geometry.fits {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window, reduce zoom, or hide blocks via Ctrl+B.",
                "Увеличьте окно, уменьшите масштаб или скройте блоки через Ctrl+B.",
            ))
            .style(palette.text()),
            area,
        );
        return;
    }
    for (panel, rect) in app.view.workspace.areas.clone() {
        let inner = chrome(frame, app, panel, rect, compact, palette);
        if app
            .settings
            .preferences
            .workspace
            .collapsed
            .contains(&panel)
        {
            continue;
        }
        match panel {
            Panel::Player => player::player(frame, app, inner, palette),
            Panel::Playlists => library::playlists(frame, app, inner, palette),
            Panel::Tracks => library::tracks_table(frame, app, inner, palette),
            _ => {}
        }
    }
    // Register shared borders after content; leave the title buttons usable at intersections.
    for (index, split) in app
        .view
        .workspace
        .splits
        .iter()
        .enumerate()
        .filter(|_| app.view.layout_editing)
    {
        let rect = resize_handle(split, compact);
        app.view.hits.push(Hit {
            area: rect.intersection(area),
            target: Target::PanelResize(index),
            enabled: split.minimum.0.saturating_add(split.minimum.1)
                < if split.axis == Axis::Horizontal {
                    split.area.width
                } else {
                    split.area.height
                },
        });
    }
    if let Some((panel, edge)) = app.view.workspace.drop
        && let Some((_, rect)) = app
            .view
            .workspace
            .areas
            .iter()
            .find(|(value, _)| *value == panel)
    {
        let preview = drop_preview(*rect, edge);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(palette.accent)),
            preview,
        );
    }
}
