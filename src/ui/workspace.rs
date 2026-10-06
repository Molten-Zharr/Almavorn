use super::{buttons::quiet_button, library, player, theme::Palette, widgets::block};
use crate::{
    app::{App, CommandMenu, Hit, Target},
    input::Action,
    model::Placement,
    workspace::{Axis, Dock, Edge, Panel, SplitArea},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

fn automatic(app: &App, area: Rect) -> Dock {
    let list_horizontal = area.width >= 55
        && matches!(
            app.settings.playlist_placement,
            Placement::Left | Placement::Right
        );
    let list_first = matches!(
        app.settings.playlist_placement,
        Placement::Left | Placement::Top
    );
    let list_ratio = if list_horizontal { 220 } else { 200 };
    let (first, second) = if list_first {
        (Panel::Playlists, Panel::Tracks)
    } else {
        (Panel::Tracks, Panel::Playlists)
    };
    let library = Dock::split(
        if list_horizontal {
            Axis::Horizontal
        } else {
            Axis::Vertical
        },
        if list_first {
            list_ratio
        } else {
            1000 - list_ratio
        },
        Dock::Panel(first),
        Dock::Panel(second),
    );
    let side_player = area.width >= 80
        && matches!(
            app.settings.player_placement,
            Placement::Left | Placement::Right
        );
    let player_first = matches!(
        app.settings.player_placement,
        Placement::Left | Placement::Top
    );
    let player_ratio = if side_player {
        250
    } else {
        (u32::from(player::preferred_height(app, area.width)) * 1000
            / u32::from(area.height.max(1))) as u16
    };
    let player = Dock::Panel(Panel::Player);
    if player_first {
        Dock::split(
            if side_player {
                Axis::Horizontal
            } else {
                Axis::Vertical
            },
            player_ratio,
            player,
            library,
        )
    } else {
        Dock::split(
            if side_player {
                Axis::Horizontal
            } else {
                Axis::Vertical
            },
            1000u16.saturating_sub(player_ratio),
            library,
            player,
        )
    }
}

fn min_width(app: &App, node: &Dock) -> u16 {
    match node {
        Dock::Panel(panel) => {
            if app.settings.workspace.hidden.contains(panel) {
                0
            } else {
                match panel {
                    Panel::Playlists => 18,
                    Panel::Tracks => 30,
                    _ => 20,
                }
            }
        }
        Dock::Split {
            axis,
            first,
            second,
            ..
        } => {
            let (left, right) = (min_width(app, first), min_width(app, second));
            if *axis == Axis::Horizontal {
                left.saturating_add(right)
            } else {
                left.max(right)
            }
        }
    }
}

fn axis(app: &App, desired: Axis, first: &Dock, second: &Dock, width: u16) -> Axis {
    if desired == Axis::Horizontal
        && min_width(app, first).saturating_add(min_width(app, second)) > width
    {
        Axis::Vertical
    } else {
        desired
    }
}

fn minimum(app: &App, node: &Dock, width: u16, compact: bool) -> u16 {
    match node {
        Dock::Panel(panel) => {
            if app.settings.workspace.hidden.contains(panel) {
                return 0;
            }
            if app.settings.workspace.collapsed.contains(panel) {
                return if compact { 1 } else { 2 };
            }
            if *panel == Panel::Player {
                return player::content_height(app, width.saturating_sub(2))
                    + if compact { 1 } else { 2 };
            }
            match panel {
                Panel::Playlists => {
                    if compact {
                        3
                    } else {
                        4
                    }
                }
                Panel::Tracks => {
                    if compact {
                        4
                    } else {
                        5
                    }
                }
                Panel::Player => {
                    if compact {
                        5
                    } else {
                        6
                    }
                }
                _ => 3,
            }
        }
        Dock::Split {
            axis: desired,
            ratio,
            first,
            second,
        } => {
            if min_width(app, first) == 0 {
                return minimum(app, second, width, compact);
            }
            if min_width(app, second) == 0 {
                return minimum(app, first, width, compact);
            }
            if axis(app, *desired, first, second, width) == Axis::Vertical {
                minimum(app, first, width, compact)
                    .saturating_add(minimum(app, second, width, compact))
            } else {
                let cut = width_cut(app, width, *ratio, first, second);
                minimum(app, first, cut, compact).max(minimum(
                    app,
                    second,
                    width.saturating_sub(cut),
                    compact,
                ))
            }
        }
    }
}

fn width_cut(app: &App, width: u16, ratio: u16, first: &Dock, second: &Dock) -> u16 {
    let left = min_width(app, first);
    let right = min_width(app, second);
    ((u32::from(width) * u32::from(ratio) / 1000) as u16).clamp(
        left.min(width),
        width.saturating_sub(right).max(left.min(width)),
    )
}

fn arrange(app: &mut App, node: &Dock, area: Rect, compact: bool, path: &mut Vec<bool>) {
    match node {
        Dock::Panel(panel) => {
            if !app.settings.workspace.hidden.contains(panel) {
                app.view.workspace.areas.push((*panel, area));
            }
        }
        Dock::Split {
            axis: desired,
            ratio,
            first,
            second,
        } => {
            if min_width(app, first) == 0 {
                path.push(true);
                arrange(app, second, area, compact, path);
                path.pop();
                return;
            }
            if min_width(app, second) == 0 {
                path.push(false);
                arrange(app, first, area, compact, path);
                path.pop();
                return;
            }
            let axis = axis(app, *desired, first, second, area.width);
            let (length, left, right) = if axis == Axis::Horizontal {
                (area.width, min_width(app, first), min_width(app, second))
            } else {
                (
                    area.height,
                    minimum(app, first, area.width, compact),
                    minimum(app, second, area.width, compact),
                )
            };
            let first_collapsed = all_collapsed(app, first);
            let second_collapsed = all_collapsed(app, second);
            let wanted = if first_collapsed {
                left
            } else if second_collapsed {
                length.saturating_sub(right)
            } else {
                (u32::from(length) * u32::from(*ratio) / 1000) as u16
            };
            let cut = wanted.clamp(
                left.min(length),
                length.saturating_sub(right).max(left.min(length)),
            );
            app.view.workspace.splits.push(SplitArea {
                path: path.clone(),
                area,
                axis,
                cut,
                minimum: (left, right),
            });
            let (a, b) = if axis == Axis::Horizontal {
                (
                    Rect::new(area.x, area.y, cut, area.height),
                    Rect::new(
                        area.x + cut,
                        area.y,
                        area.width.saturating_sub(cut),
                        area.height,
                    ),
                )
            } else {
                (
                    Rect::new(area.x, area.y, area.width, cut),
                    Rect::new(
                        area.x,
                        area.y + cut,
                        area.width,
                        area.height.saturating_sub(cut),
                    ),
                )
            };
            path.push(false);
            arrange(app, first, a, compact, path);
            path.pop();
            path.push(true);
            arrange(app, second, b, compact, path);
            path.pop();
        }
    }
}

fn all_collapsed(app: &App, node: &Dock) -> bool {
    match node {
        Dock::Panel(panel) => {
            app.settings.workspace.hidden.contains(panel)
                || app.settings.workspace.collapsed.contains(panel)
        }
        Dock::Split { first, second, .. } => {
            all_collapsed(app, first) && all_collapsed(app, second)
        }
    }
}

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
    let title_width = area
        .width
        .saturating_sub(if app.view.layout_editing { 13 } else { 9 });
    let heading = if panel == Panel::Tracks {
        app.playlist()
            .map(|playlist| playlist.display_name(app.settings.language))
            .unwrap_or(panel.name(app.settings.language))
    } else {
        panel.name(app.settings.language)
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
            quiet_button(
                frame,
                app,
                Rect::new(area.right().saturating_sub(8), area.y, 3, 1).intersection(area),
                "+",
                Target::Action(Action::NewPlaylist),
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
    let collapsed = app.settings.workspace.collapsed.contains(&panel);
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
    let root = app
        .settings
        .workspace
        .root
        .clone()
        .and_then(Dock::content_only)
        .unwrap_or_else(|| automatic(app, area));
    let compact = minimum(app, &root, area.width, false) > area.height;
    if minimum(app, &root, area.width, compact) > area.height {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window, reduce zoom, or hide blocks via Ctrl+B.",
                "Увеличьте окно, уменьшите масштаб или скройте блоки через Ctrl+B.",
            ))
            .style(palette.text()),
            area,
        );
        app.view.workspace.root = Some(root);
        return;
    }
    arrange(app, &root, area, compact, &mut Vec::new());
    app.view.workspace.root = Some(root);
    for (panel, rect) in app.view.workspace.areas.clone() {
        let inner = chrome(frame, app, panel, rect, compact, palette);
        if app.settings.workspace.collapsed.contains(&panel) {
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
        let rect = match split.axis {
            Axis::Horizontal => Rect::new(
                split.area.x + split.cut.saturating_sub(1),
                split.area.y + 1,
                2,
                split.area.height.saturating_sub(1),
            ),
            Axis::Vertical => Rect::new(
                split.area.x,
                split.area.y
                    + if compact {
                        split.cut
                    } else {
                        split.cut.saturating_sub(1)
                    },
                split.area.width,
                if compact { 1 } else { 2 },
            ),
        };
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
        let mut preview = *rect;
        match edge {
            Edge::Left => preview.width /= 2,
            Edge::Right => {
                preview.x += preview.width / 2;
                preview.width -= preview.width / 2;
            }
            Edge::Top => preview.height /= 2,
            Edge::Bottom => {
                preview.y += preview.height / 2;
                preview.height -= preview.height / 2;
            }
            Edge::Center => {}
        }
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(palette.accent)),
            preview,
        );
    }
}

pub(super) fn fits(app: &App, root: &Dock) -> bool {
    minimum(app, root, app.view.workspace.bounds.width, true) <= app.view.workspace.bounds.height
}
