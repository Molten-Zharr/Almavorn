use super::{
    library, player,
    theme::Palette,
    toolbar::{self, Group},
    widgets::{block, button_rows, button_width},
};
use crate::{
    app::{App, Hit, Target},
    model::Placement,
    workspace::{Axis, Dock, Edge, Panel, SplitArea},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};

type ToolbarRow = (Vec<(Dock, u16)>, u16, u16);

fn join(axis: Axis, mut nodes: Vec<(Dock, u16)>) -> Dock {
    if nodes.len() == 1 {
        return nodes.remove(0).0;
    }
    let total: u32 = nodes.iter().map(|(_, size)| u32::from(*size)).sum();
    let (first, size) = nodes.remove(0);
    Dock::split(
        axis,
        (u32::from(size) * 1000 / total.max(1)) as u16,
        first,
        join(axis, nodes),
    )
}

fn automatic(app: &App, groups: &[Group], area: Rect, compact: bool) -> Dock {
    let (rects, height) = toolbar::layout(app, &groups[..6], area.width, compact);
    let mut rows: Vec<ToolbarRow> = Vec::new();
    for (index, rect) in rects.iter().enumerate() {
        if rows.last().is_none_or(|(_, y, _)| *y != rect.y) {
            rows.push((Vec::new(), rect.y, rect.height));
        }
        let row = rows.last_mut().expect("Toolbar row");
        row.0.push((Dock::Panel(Panel::ALL[index]), rect.width));
        row.2 = row.2.max(rect.height);
    }
    let toolbar = join(
        Axis::Vertical,
        rows.into_iter()
            .map(|(nodes, _, size)| (join(Axis::Horizontal, nodes), size))
            .collect(),
    );
    let list_horizontal = area.width >= 70
        && matches!(
            app.settings.playlist_placement,
            Placement::Left | Placement::Right
        );
    let list_first = matches!(
        app.settings.playlist_placement,
        Placement::Left | Placement::Top
    );
    let list_ratio = if list_horizontal { 240 } else { 250 };
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
            / u32::from(area.height.saturating_sub(height).max(1))) as u16
    };
    let player = Dock::split(
        Axis::Vertical,
        400,
        Dock::Panel(Panel::Player),
        Dock::split(
            Axis::Horizontal,
            650,
            Dock::Panel(Panel::Playback),
            Dock::Panel(Panel::Volume),
        ),
    );
    let content = if player_first {
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
    };
    Dock::split(
        Axis::Vertical,
        (u32::from(height) * 1000 / u32::from(area.height.max(1))) as u16,
        toolbar,
        content,
    )
}

fn group(groups: &[Group], panel: Panel) -> Option<&Group> {
    match panel {
        Panel::Playback => groups.get(6),
        Panel::Volume => groups.get(7),
        _ => Panel::ALL
            .iter()
            .position(|value| *value == panel)
            .and_then(|index| (index < 6).then(|| &groups[index])),
    }
}

fn min_width(app: &App, groups: &[Group], node: &Dock) -> u16 {
    match node {
        Dock::Panel(panel) => {
            if app.settings.workspace.hidden.contains(panel) {
                0
            } else if let Some(group) = group(groups, *panel) {
                group
                    .entries
                    .iter()
                    .map(|(label, target, _)| button_width(app, label, target).saturating_add(2))
                    .max()
                    .unwrap_or(20)
                    .max(20)
                    .max(panel.name(app.settings.language).chars().count() as u16 + 13)
            } else if *panel == Panel::Playlists {
                28
            } else if *panel == Panel::Tracks {
                42
            } else {
                20
            }
        }
        Dock::Split {
            axis,
            first,
            second,
            ..
        } => {
            let (left, right) = (
                min_width(app, groups, first),
                min_width(app, groups, second),
            );
            if *axis == Axis::Horizontal {
                left.saturating_add(right)
            } else {
                left.max(right)
            }
        }
    }
}

fn axis(
    app: &App,
    groups: &[Group],
    desired: Axis,
    first: &Dock,
    second: &Dock,
    width: u16,
) -> Axis {
    if desired == Axis::Horizontal
        && min_width(app, groups, first).saturating_add(min_width(app, groups, second)) > width
    {
        Axis::Vertical
    } else {
        desired
    }
}

fn minimum(app: &App, groups: &[Group], node: &Dock, width: u16, compact: bool) -> u16 {
    match node {
        Dock::Panel(panel) => {
            if app.settings.workspace.hidden.contains(panel) {
                return 0;
            }
            if app.settings.workspace.collapsed.contains(panel) {
                return if compact { 1 } else { 2 };
            }
            if let Some(group) = group(groups, *panel) {
                return button_rows(app, width.saturating_sub(2), &group.entries)
                    + if compact { 1 } else { 2 };
            }
            match panel {
                Panel::Playlists => {
                    if compact {
                        4
                    } else {
                        5
                    }
                }
                Panel::Tracks => {
                    if compact {
                        6
                    } else {
                        7
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
            if min_width(app, groups, first) == 0 {
                return minimum(app, groups, second, width, compact);
            }
            if min_width(app, groups, second) == 0 {
                return minimum(app, groups, first, width, compact);
            }
            if axis(app, groups, *desired, first, second, width) == Axis::Vertical {
                minimum(app, groups, first, width, compact)
                    .saturating_add(minimum(app, groups, second, width, compact))
            } else {
                let cut = width_cut(app, groups, width, *ratio, first, second);
                minimum(app, groups, first, cut, compact).max(minimum(
                    app,
                    groups,
                    second,
                    width.saturating_sub(cut),
                    compact,
                ))
            }
        }
    }
}

fn width_cut(
    app: &App,
    groups: &[Group],
    width: u16,
    ratio: u16,
    first: &Dock,
    second: &Dock,
) -> u16 {
    let left = min_width(app, groups, first);
    let right = min_width(app, groups, second);
    ((u32::from(width) * u32::from(ratio) / 1000) as u16).clamp(
        left.min(width),
        width.saturating_sub(right).max(left.min(width)),
    )
}

fn arrange(
    app: &mut App,
    groups: &[Group],
    node: &Dock,
    area: Rect,
    compact: bool,
    path: &mut Vec<bool>,
) {
    match node {
        Dock::Panel(panel) => {
            if !app.settings.workspace.hidden.contains(panel) {
                app.workspace.areas.push((*panel, area));
            }
        }
        Dock::Split {
            axis: desired,
            ratio,
            first,
            second,
        } => {
            if min_width(app, groups, first) == 0 {
                path.push(true);
                arrange(app, groups, second, area, compact, path);
                path.pop();
                return;
            }
            if min_width(app, groups, second) == 0 {
                path.push(false);
                arrange(app, groups, first, area, compact, path);
                path.pop();
                return;
            }
            let axis = axis(app, groups, *desired, first, second, area.width);
            let (length, left, right) = if axis == Axis::Horizontal {
                (
                    area.width,
                    min_width(app, groups, first),
                    min_width(app, groups, second),
                )
            } else {
                (
                    area.height,
                    minimum(app, groups, first, area.width, compact),
                    minimum(app, groups, second, area.width, compact),
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
            app.workspace.splits.push(SplitArea {
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
            arrange(app, groups, first, a, compact, path);
            path.pop();
            path.push(true);
            arrange(app, groups, second, b, compact, path);
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
    let active = app.workspace.focus == panel;
    let outer = block(String::new(), palette, active)
        .borders(if compact {
            Borders::TOP | Borders::LEFT | Borders::RIGHT
        } else {
            Borders::ALL
        })
        .border_style(Style::default().fg(if active {
            palette.accent
        } else if panel == Panel::Tracks {
            palette.inactive_file_border
        } else {
            palette.inactive_panel_border
        }));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    app.hits.push(Hit {
        area,
        target: Target::PanelFocus(panel),
        enabled: true,
    });
    let title_width = area.width.saturating_sub(13);
    let title = panel
        .name(app.settings.language)
        .chars()
        .take(usize::from(title_width))
        .collect::<String>();
    let title_area = Rect::new(area.x + 1, area.y, title_width, 1);
    let title_hit = Rect::new(title_area.x, title_area.y, title.chars().count() as u16, 1);
    frame.render_widget(
        Paragraph::new(title).style(
            palette
                .text()
                .fg(if active {
                    palette.accent
                } else {
                    palette.muted
                })
                .add_modifier(Modifier::BOLD),
        ),
        title_area,
    );
    app.hits.push(Hit {
        area: title_hit,
        target: Target::PanelMove(panel),
        enabled: true,
    });
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
        frame.render_widget(
            Paragraph::new(label).style(
                palette
                    .text()
                    .fg(if active || app.hovered(rect) {
                        palette.accent
                    } else {
                        palette.muted
                    })
                    .bg(if app.hovered(rect) {
                        palette.selection
                    } else {
                        palette.background
                    }),
            ),
            rect,
        );
        app.hits.push(Hit {
            area: rect,
            target,
            enabled: true,
        });
    }
    inner
}

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.workspace.bounds = area;
    app.workspace.areas.clear();
    app.workspace.splits.clear();
    app.playlist_area = Rect::default();
    app.tracks_area = Rect::default();
    let mut groups = toolbar::commands(app);
    groups.extend(player::controls(app));
    let mut root = app
        .settings
        .workspace
        .root
        .clone()
        .unwrap_or_else(|| automatic(app, &groups, area, false));
    let compact = minimum(app, &groups, &root, area.width, false) > area.height;
    if compact && app.settings.workspace.root.is_none() {
        root = automatic(app, &groups, area, true);
    }
    if minimum(app, &groups, &root, area.width, compact) > area.height {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window, reduce zoom, or hide blocks via Ctrl+B.",
                "Увеличьте окно, уменьшите масштаб или скройте блоки через Ctrl+B.",
            ))
            .style(palette.text()),
            area,
        );
        app.workspace.root = Some(root);
        return;
    }
    arrange(app, &groups, &root, area, compact, &mut Vec::new());
    app.workspace.root = Some(root);
    for (panel, rect) in app.workspace.areas.clone() {
        let inner = chrome(frame, app, panel, rect, compact, palette);
        if app.settings.workspace.collapsed.contains(&panel) {
            continue;
        }
        if let Some(group) = group(&groups, panel) {
            toolbar::contents(frame, app, inner, group, palette);
        } else {
            match panel {
                Panel::Playlists => library::playlists(frame, app, inner, palette),
                Panel::Tracks => library::tracks_table(frame, app, inner, palette),
                Panel::Player => player::player(frame, app, inner, palette),
                _ => {}
            }
        }
    }
    // Register shared borders after content; leave the title buttons usable at intersections.
    for (index, split) in app.workspace.splits.iter().enumerate() {
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
        app.hits.push(Hit {
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
    if let Some((panel, edge)) = app.workspace.drop
        && let Some((_, rect)) = app
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
    let mut groups = toolbar::commands(app);
    groups.extend(player::controls(app));
    minimum(app, &groups, root, app.workspace.bounds.width, true) <= app.workspace.bounds.height
}
