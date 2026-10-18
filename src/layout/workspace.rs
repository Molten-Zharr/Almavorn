use super::player::PlayerMetrics;
use crate::{
    model::Placement,
    workspace::{Axis, Dock, Edge, Panel, SplitArea},
};
use ratatui::layout::Rect;

pub(crate) struct WorkspaceMetrics {
    pub playlist_placement: Placement,
    pub player_placement: Placement,
    pub hidden: Vec<Panel>,
    pub collapsed: Vec<Panel>,
    pub playlist_min_width: u16,
    pub player: PlayerMetrics,
}

pub(crate) fn resize_handle(split: &SplitArea, compact: bool) -> Rect {
    match split.axis {
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
    }
}

pub(crate) fn drop_preview(mut area: Rect, edge: Edge) -> Rect {
    match edge {
        Edge::Left => area.width /= 2,
        Edge::Right => {
            area.x += area.width / 2;
            area.width -= area.width / 2;
        }
        Edge::Top => area.height /= 2,
        Edge::Bottom => {
            area.y += area.height / 2;
            area.height -= area.height / 2;
        }
        Edge::Center => {}
    }
    area
}

pub(crate) struct WorkspaceGeometry {
    pub root: Dock,
    pub compact: bool,
    pub fits: bool,
    pub areas: Vec<(Panel, Rect)>,
    pub splits: Vec<SplitArea>,
}

impl WorkspaceMetrics {
    pub fn arrange(&self, root: Option<&Dock>, area: Rect) -> WorkspaceGeometry {
        let root = root
            .cloned()
            .and_then(Dock::content_only)
            .unwrap_or_else(|| self.automatic(area));
        let compact = self.minimum(&root, area.width, false) > area.height;
        let fits = self.minimum(&root, area.width, compact) <= area.height;
        let mut geometry = WorkspaceGeometry {
            root,
            compact,
            fits,
            areas: Vec::new(),
            splits: Vec::new(),
        };
        if fits {
            let root = geometry.root.clone();
            self.arrange_node(&root, area, compact, &mut geometry, &mut Vec::new());
        }
        geometry
    }

    pub fn fits(&self, root: &Dock, area: Rect) -> bool {
        self.minimum(root, area.width, true) <= area.height
    }

    fn automatic(&self, area: Rect) -> Dock {
        let list_horizontal = area.width >= 55
            && matches!(self.playlist_placement, Placement::Left | Placement::Right);
        let list_first = matches!(self.playlist_placement, Placement::Left | Placement::Top);
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
        let side_player =
            area.width >= 80 && matches!(self.player_placement, Placement::Left | Placement::Right);
        let player_first = matches!(self.player_placement, Placement::Left | Placement::Top);
        let player_ratio = if side_player {
            250
        } else {
            (u32::from(self.player.preferred_height(area.width)) * 1000
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

    fn min_width(&self, node: &Dock) -> u16 {
        match node {
            Dock::Panel(panel) => {
                if self.hidden.contains(panel) {
                    0
                } else {
                    match panel {
                        Panel::Playlists => self.playlist_min_width,
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
                let (left, right) = (self.min_width(first), self.min_width(second));
                if *axis == Axis::Horizontal {
                    left.saturating_add(right)
                } else {
                    left.max(right)
                }
            }
        }
    }

    fn axis(&self, desired: Axis, first: &Dock, second: &Dock, width: u16) -> Axis {
        if desired == Axis::Horizontal
            && self.min_width(first).saturating_add(self.min_width(second)) > width
        {
            Axis::Vertical
        } else {
            desired
        }
    }

    fn minimum(&self, node: &Dock, width: u16, compact: bool) -> u16 {
        match node {
            Dock::Panel(panel) => {
                if self.hidden.contains(panel) {
                    return 0;
                }
                if self.collapsed.contains(panel) {
                    return if compact { 1 } else { 2 };
                }
                if *panel == Panel::Player {
                    return self.player.content_height(width.saturating_sub(2))
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
                if self.min_width(first) == 0 {
                    return self.minimum(second, width, compact);
                }
                if self.min_width(second) == 0 {
                    return self.minimum(first, width, compact);
                }
                if self.axis(*desired, first, second, width) == Axis::Vertical {
                    self.minimum(first, width, compact)
                        .saturating_add(self.minimum(second, width, compact))
                } else {
                    let cut = self.width_cut(width, *ratio, first, second);
                    self.minimum(first, cut, compact).max(self.minimum(
                        second,
                        width.saturating_sub(cut),
                        compact,
                    ))
                }
            }
        }
    }

    fn width_cut(&self, width: u16, ratio: u16, first: &Dock, second: &Dock) -> u16 {
        let left = self.min_width(first);
        let right = self.min_width(second);
        ((u32::from(width) * u32::from(ratio) / 1000) as u16).clamp(
            left.min(width),
            width.saturating_sub(right).max(left.min(width)),
        )
    }

    fn arrange_node(
        &self,
        node: &Dock,
        area: Rect,
        compact: bool,
        geometry: &mut WorkspaceGeometry,
        path: &mut Vec<bool>,
    ) {
        match node {
            Dock::Panel(panel) => {
                if !self.hidden.contains(panel) {
                    geometry.areas.push((*panel, area));
                }
            }
            Dock::Split {
                axis: desired,
                ratio,
                first,
                second,
            } => {
                if self.min_width(first) == 0 {
                    path.push(true);
                    self.arrange_node(second, area, compact, geometry, path);
                    path.pop();
                    return;
                }
                if self.min_width(second) == 0 {
                    path.push(false);
                    self.arrange_node(first, area, compact, geometry, path);
                    path.pop();
                    return;
                }
                let axis = self.axis(*desired, first, second, area.width);
                let (length, left, right) = if axis == Axis::Horizontal {
                    (area.width, self.min_width(first), self.min_width(second))
                } else {
                    (
                        area.height,
                        self.minimum(first, area.width, compact),
                        self.minimum(second, area.width, compact),
                    )
                };
                let first_collapsed = self.all_collapsed(first);
                let second_collapsed = self.all_collapsed(second);
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
                geometry.splits.push(SplitArea {
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
                self.arrange_node(first, a, compact, geometry, path);
                path.pop();
                path.push(true);
                self.arrange_node(second, b, compact, geometry, path);
                path.pop();
            }
        }
    }

    fn all_collapsed(&self, node: &Dock) -> bool {
        match node {
            Dock::Panel(panel) => self.hidden.contains(panel) || self.collapsed.contains(panel),
            Dock::Split { first, second, .. } => {
                self.all_collapsed(first) && self.all_collapsed(second)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{input::Action, layout::player::PlayerControl};

    fn metrics() -> WorkspaceMetrics {
        WorkspaceMetrics {
            playlist_placement: Placement::Left,
            player_placement: Placement::Bottom,
            hidden: Vec::new(),
            collapsed: Vec::new(),
            playlist_min_width: 22,
            player: PlayerMetrics::default(),
        }
    }

    fn library() -> Dock {
        Dock::split(
            Axis::Horizontal,
            500,
            Dock::Panel(Panel::Playlists),
            Dock::Panel(Panel::Tracks),
        )
    }

    #[test]
    fn narrow_library_adapts_to_vertical_then_compact_then_reports_insufficient_space() {
        let metrics = metrics();
        let root = library();
        let wide = metrics.arrange(Some(&root), Rect::new(7, 11, 52, 9));
        assert!(!wide.compact);
        assert_eq!(wide.splits[0].axis, Axis::Horizontal);
        assert_eq!(
            wide.areas,
            [
                (Panel::Playlists, Rect::new(7, 11, 22, 9)),
                (Panel::Tracks, Rect::new(29, 11, 30, 9))
            ]
        );
        let narrow = metrics.arrange(Some(&root), Rect::new(7, 11, 51, 9));
        assert_eq!(narrow.splits[0].axis, Axis::Vertical);
        assert_eq!(
            narrow.areas,
            [
                (Panel::Playlists, Rect::new(7, 11, 51, 4)),
                (Panel::Tracks, Rect::new(7, 15, 51, 5))
            ]
        );
        let compact = metrics.arrange(Some(&root), Rect::new(7, 11, 51, 7));
        assert!(compact.compact && compact.fits);
        assert_eq!(compact.areas[0].1.height, 3);
        assert_eq!(compact.areas[1].1.height, 4);
        let too_small = metrics.arrange(Some(&root), Rect::new(7, 11, 51, 6));
        assert!(!too_small.fits);
        assert!(!metrics.fits(&root, Rect::new(7, 11, 51, 6)));
        assert!(too_small.areas.is_empty() && too_small.splits.is_empty());
        assert_eq!(too_small.root, root);
    }

    #[test]
    fn hiding_a_parent_branch_preserves_the_remaining_split_path_for_resizing() {
        let mut metrics = metrics();
        metrics.hidden = vec![Panel::Player];
        let root = Dock::split(Axis::Vertical, 500, Dock::Panel(Panel::Player), library());
        let area = Rect::new(7, 11, 100, 30);
        let geometry = metrics.arrange(Some(&root), area);
        assert!(geometry.fits);
        assert_eq!(geometry.splits.len(), 1);
        assert_eq!(geometry.splits[0].path, [true]);
        assert_eq!(geometry.splits[0].area, area);
        assert_eq!(geometry.areas.len(), 2);
        metrics.hidden.push(Panel::Playlists);
        let geometry = metrics.arrange(Some(&root), area);
        assert!(geometry.splits.is_empty());
        assert_eq!(geometry.areas, [(Panel::Tracks, area)]);
    }

    #[test]
    fn collapsed_panels_keep_only_their_heading_and_give_remaining_space_to_content() {
        let mut metrics = metrics();
        metrics.collapsed = vec![Panel::Playlists];
        let root = Dock::split(
            Axis::Vertical,
            900,
            Dock::Panel(Panel::Playlists),
            Dock::Panel(Panel::Tracks),
        );
        let geometry = metrics.arrange(Some(&root), Rect::new(0, 0, 80, 30));
        assert!(geometry.fits && !geometry.compact);
        assert_eq!(
            geometry.areas,
            [
                (Panel::Playlists, Rect::new(0, 0, 80, 2)),
                (Panel::Tracks, Rect::new(0, 2, 80, 28))
            ]
        );
        let compact = metrics.arrange(Some(&root), Rect::new(0, 0, 80, 5));
        assert!(compact.compact && compact.fits);
        assert_eq!(compact.areas[0].1.height, 1);
        assert_eq!(compact.areas[1].1.height, 4);
    }

    #[test]
    fn resize_handles_and_drop_previews_keep_titles_reachable_and_cover_the_requested_edge() {
        let split = SplitArea {
            path: Vec::new(),
            area: Rect::new(10, 20, 40, 30),
            axis: Axis::Horizontal,
            cut: 12,
            minimum: (8, 8),
        };
        assert_eq!(resize_handle(&split, false), Rect::new(21, 21, 2, 29));
        let vertical = SplitArea {
            axis: Axis::Vertical,
            ..split
        };
        assert_eq!(resize_handle(&vertical, false), Rect::new(10, 31, 40, 2));
        assert_eq!(resize_handle(&vertical, true), Rect::new(10, 32, 40, 1));
        assert_eq!(
            drop_preview(vertical.area, Edge::Left),
            Rect::new(10, 20, 20, 30)
        );
        assert_eq!(
            drop_preview(vertical.area, Edge::Right),
            Rect::new(30, 20, 20, 30)
        );
        assert_eq!(
            drop_preview(vertical.area, Edge::Top),
            Rect::new(10, 20, 40, 15)
        );
        assert_eq!(
            drop_preview(vertical.area, Edge::Bottom),
            Rect::new(10, 35, 40, 15)
        );
        assert_eq!(drop_preview(vertical.area, Edge::Center), vertical.area);
    }

    #[test]
    fn plans_partition_the_available_area_and_agree_with_fit_checks_across_layout_states() {
        let placements = [
            Placement::Left,
            Placement::Right,
            Placement::Top,
            Placement::Bottom,
        ];
        let custom = Dock::split(
            Axis::Horizontal,
            999,
            Dock::Panel(Panel::Player),
            Dock::split(
                Axis::Vertical,
                1,
                Dock::Panel(Panel::Tracks),
                Dock::Panel(Panel::Playlists),
            ),
        );
        for playlist_placement in placements {
            for player_placement in placements {
                for hidden in 0..8 {
                    for collapsed in [0, 1, 3, 7] {
                        let mut metrics = metrics();
                        metrics.playlist_placement = playlist_placement;
                        metrics.player_placement = player_placement;
                        metrics.hidden = Panel::CONTENT
                            .into_iter()
                            .enumerate()
                            .filter_map(|(index, panel)| {
                                (hidden & (1 << index) != 0).then_some(panel)
                            })
                            .collect();
                        metrics.collapsed = Panel::CONTENT
                            .into_iter()
                            .enumerate()
                            .filter_map(|(index, panel)| {
                                (collapsed & (1 << index) != 0).then_some(panel)
                            })
                            .collect();
                        metrics.player.checked_loop = true;
                        metrics.player.waveform = true;
                        metrics.player.controls = [8, 10, 8, 8, 12, 9, 8, 8, 10]
                            .into_iter()
                            .map(|width| PlayerControl {
                                action: Action::TogglePlay,
                                label: "play",
                                width,
                            })
                            .collect();
                        for width in [32, 54, 55, 69, 70, 79, 80, 120] {
                            for height in [6, 12, 26, 45] {
                                let area = Rect::new(7, 11, width, height);
                                for root in [None, Some(&custom)] {
                                    let geometry = metrics.arrange(root, area);
                                    assert_eq!(geometry.fits, metrics.fits(&geometry.root, area));
                                    if !geometry.fits {
                                        assert!(
                                            geometry.areas.is_empty() && geometry.splits.is_empty()
                                        );
                                        continue;
                                    }
                                    assert_eq!(geometry.areas.len(), 3 - metrics.hidden.len());
                                    let mut covered = 0u32;
                                    for (index, (panel, rect)) in geometry.areas.iter().enumerate()
                                    {
                                        assert!(!metrics.hidden.contains(panel));
                                        assert_eq!(rect.intersection(area), *rect);
                                        covered += u32::from(rect.width) * u32::from(rect.height);
                                        for (_, other) in &geometry.areas[index + 1..] {
                                            assert!(rect.intersection(*other).is_empty());
                                        }
                                    }
                                    if !geometry.areas.is_empty() {
                                        assert_eq!(covered, u32::from(width) * u32::from(height));
                                    }
                                    let mut root = geometry.root.clone();
                                    for split in &geometry.splits {
                                        assert!(matches!(
                                            root.at_mut(&split.path),
                                            Some(Dock::Split { .. })
                                        ));
                                        assert_eq!(split.area.intersection(area), split.area);
                                        let length = if split.axis == Axis::Horizontal {
                                            split.area.width
                                        } else {
                                            split.area.height
                                        };
                                        assert!(
                                            split.cut >= split.minimum.0
                                                && length.saturating_sub(split.cut)
                                                    >= split.minimum.1
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
