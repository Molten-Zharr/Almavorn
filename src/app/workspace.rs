use super::{App, Dialog, Focus, database::DatabaseOutcome};
use crate::{
    input::{Key, KeyPress},
    workspace::{Axis, Dock, Edge, Gesture, Panel, PanelRow, panel_rows},
};
use anyhow::Result;
use ratatui::layout::Position;

impl App {
    pub(crate) fn activate_panel_row(&mut self, index: usize, visibility: bool) -> Result<()> {
        let Some(Dialog::Panels { selected, expanded }) = &mut self.view.dialog else {
            return Ok(());
        };
        let rows = panel_rows(expanded);
        let Some(row) = rows.get(index).copied() else {
            return Ok(());
        };
        *selected = index;
        match row {
            PanelRow::Category(panel) => {
                if visibility || panel.controls().is_empty() {
                    self.show_panel(panel)?;
                } else {
                    let open = !expanded.contains(&panel);
                    self.expand_panel_category(open);
                }
            }
            PanelRow::Control(control) => {
                if self.settings.workspace.hidden_controls.contains(&control) {
                    self.settings
                        .workspace
                        .hidden_controls
                        .retain(|value| *value != control);
                } else {
                    self.settings.workspace.hidden_controls.push(control);
                }
                self.save_settings()?;
            }
            PanelRow::Reset => {
                self.reset_workspace()?;
                if let Some(Dialog::Panels { selected, expanded }) = &mut self.view.dialog {
                    expanded.clear();
                    *selected = Panel::ALL.len();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn expand_panel_category(&mut self, open: bool) {
        let Some(Dialog::Panels { selected, expanded }) = &mut self.view.dialog else {
            return;
        };
        let rows = panel_rows(expanded);
        let category = match rows.get(*selected) {
            Some(PanelRow::Category(panel)) => Some(*panel),
            Some(PanelRow::Control(control)) if !open => Panel::ALL
                .into_iter()
                .find(|panel| panel.controls().contains(control)),
            _ => None,
        };
        if let Some(panel) = category.filter(|panel| !panel.controls().is_empty()) {
            if open && !expanded.contains(&panel) {
                expanded.push(panel);
            } else if !open {
                expanded.retain(|value| *value != panel);
            }
            *selected = panel_rows(expanded)
                .iter()
                .position(|row| matches!(row, PanelRow::Category(value) if *value == panel))
                .unwrap_or(0);
        }
    }
    pub(crate) fn focus_panel(&mut self, panel: Panel) {
        self.view.toolbar_selected = None;
        self.view.workspace.focus = panel;
        match panel {
            Panel::Playlists => self.view.focus = Focus::Playlists,
            Panel::Tracks => self.view.focus = Focus::Tracks,
            _ => {}
        }
    }

    pub(crate) fn focus_point(&mut self, point: Position) {
        if let Some(panel) = self
            .view
            .workspace
            .areas
            .iter()
            .find(|(_, area)| area.contains(point))
            .map(|(panel, _)| *panel)
        {
            self.focus_panel(panel);
        }
    }

    pub(crate) fn close_panel(&mut self, panel: Panel) -> Result<()> {
        if !self.settings.workspace.hidden.contains(&panel) {
            self.settings.workspace.hidden.push(panel);
        }
        if self.view.workspace.focus == panel {
            let index = self
                .view
                .workspace
                .areas
                .iter()
                .position(|(value, _)| *value == panel)
                .unwrap_or(0);
            let next = self
                .view
                .workspace
                .areas
                .iter()
                .cycle()
                .skip(index + 1)
                .take(self.view.workspace.areas.len())
                .map(|(panel, _)| *panel)
                .find(|panel| !self.settings.workspace.hidden.contains(panel));
            if let Some(next) = next {
                self.focus_panel(next);
            }
        }
        self.save_settings()
    }

    pub(crate) fn toggle_panel(&mut self, panel: Panel) -> Result<()> {
        self.focus_panel(panel);
        if self.settings.workspace.collapsed.contains(&panel) {
            self.settings
                .workspace
                .collapsed
                .retain(|value| *value != panel);
        } else {
            self.settings.workspace.collapsed.push(panel);
        }
        self.save_settings()
    }

    pub(crate) fn show_panel(&mut self, panel: Panel) -> Result<()> {
        if let Some(Dialog::Panels { selected, expanded }) = &mut self.view.dialog {
            *selected = panel_rows(expanded)
                .iter()
                .position(|row| matches!(row, PanelRow::Category(value) if *value == panel))
                .unwrap_or(*selected);
        }
        if self.settings.workspace.hidden.contains(&panel) {
            self.settings
                .workspace
                .hidden
                .retain(|value| *value != panel);
            self.focus_panel(panel);
            self.save_settings()
        } else {
            self.close_panel(panel)
        }
    }

    pub(crate) fn reset_workspace(&mut self) -> Result<()> {
        self.view.workspace.gesture = None;
        self.view.workspace.drop = None;
        self.view.workspace.root = None;
        self.settings.workspace = Default::default();
        self.focus_panel(Panel::Tracks);
        self.save_settings()
    }

    pub(crate) fn drag_workspace(&mut self, point: Position) -> Result<()> {
        match &self.view.workspace.gesture {
            Some(Gesture::Playlist { id, .. }) => {
                let id = *id;
                // A wheel event changes the viewport before hit regions are redrawn.
                let target =
                    if self.view.dialog.is_none() && self.view.playlist_area.contains(point) {
                        let row = usize::from(point.y - self.view.playlist_area.y);
                        self.visible_playlists()
                            .get(self.view.playlist_offset + row)
                            .map(|playlist| playlist.id)
                            .filter(|target| *target != id && self.playlist_movable(*target))
                    } else {
                        None
                    };
                if let Some(Gesture::Playlist {
                    target: destination,
                    ..
                }) = &mut self.view.workspace.gesture
                {
                    *destination = target;
                }
            }
            Some(Gesture::Move { panel, .. }) => {
                self.view.workspace.drop = self.view.workspace.destination(point, *panel);
            }
            Some(Gesture::Resize { split, .. }) => {
                let (length, coordinate, start) = match split.axis {
                    Axis::Horizontal => (split.area.width, point.x, split.area.x),
                    Axis::Vertical => (split.area.height, point.y, split.area.y),
                };
                let cut = coordinate.saturating_sub(start).clamp(
                    split.minimum.0,
                    length.saturating_sub(split.minimum.1).max(split.minimum.0),
                );
                if let Some(mut root) = self.view.workspace.root.clone()
                    && let Some(Dock::Split { ratio, .. }) = root.at_mut(&split.path)
                {
                    *ratio =
                        ((u32::from(cut) * 1000 / u32::from(length.max(1))) as u16).clamp(1, 999);
                    if crate::ui::workspace_fits(self, &root) {
                        self.settings.workspace.root = Some(root.clone());
                        self.view.workspace.root = Some(root);
                    }
                }
            }
            Some(Gesture::Seek(area)) => {
                let area = *area;
                self.view.pointer = Position::new(
                    point.x.clamp(area.x, area.right().saturating_sub(1)),
                    area.y,
                );
                self.target(super::Target::Seek(area), false)?;
            }
            Some(Gesture::PlaybackVolume(area)) => {
                let area = *area;
                self.view.pointer = Position::new(
                    point.x.clamp(area.x, area.right().saturating_sub(1)),
                    area.y,
                );
                self.target(super::Target::PlaybackVolume(area), false)?;
            }
            Some(Gesture::Volume(index, area)) => {
                let (index, area) = (*index, *area);
                self.view.pointer = Position::new(
                    point.x.clamp(area.x, area.right().saturating_sub(1)),
                    area.y,
                );
                self.target(super::Target::SettingsVolume(index, area), false)?;
            }
            Some(Gesture::EqualizerGain {
                index,
                area,
                vertical,
            }) => {
                let (index, area, vertical) = (*index, *area, *vertical);
                self.view.pointer = Position::new(
                    point.x.clamp(area.x, area.right().saturating_sub(1)),
                    point.y.clamp(area.y, area.bottom().saturating_sub(1)),
                );
                self.equalizer_target(super::Target::EqualizerGain {
                    index,
                    area,
                    vertical,
                })?;
            }
            None => {}
        }
        Ok(())
    }

    pub(crate) fn release_workspace(&mut self, point: Position) -> Result<()> {
        self.drag_workspace(point)?;
        match self.view.workspace.gesture.take() {
            Some(Gesture::Playlist {
                id,
                target: Some(target),
            }) if self.playlist_movable(id) && self.playlist_movable(target) => {
                let unlocked = self.view.editing;
                self.start_database(None, move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.move_playlist_to(id, target, unlocked)?,
                    ))
                })?;
            }
            Some(Gesture::Move { panel, origin }) if point != origin => {
                if let Some((target, edge)) = self.view.workspace.drop.take()
                    && let Some(root) = self.view.workspace.root.take()
                {
                    let before = root.clone();
                    let mut root = root;
                    if edge == Edge::Center {
                        root.swap(panel, target);
                    } else if let Some(mut remaining) = root.clone().remove(panel) {
                        remaining.insert(panel, target, edge);
                        root = remaining;
                    }
                    if crate::ui::workspace_fits(self, &root) {
                        self.settings.workspace.root = Some(root.clone());
                        self.view.workspace.root = Some(root);
                        self.save_settings()?;
                    } else {
                        self.view.workspace.root = Some(before);
                        self.message(self.text("Not enough space for this arrangement. Enlarge the window or hide another block.", "Для этой раскладки не хватает места. Увеличьте окно или скройте другой блок.").into());
                    }
                }
            }
            Some(Gesture::Resize { .. }) => self.save_settings()?,
            Some(Gesture::Seek(_)) => self.commit_seek(),
            _ => {}
        }
        self.view.workspace.drop = None;
        Ok(())
    }

    pub(crate) fn cancel_workspace_drag(&mut self) -> bool {
        let Some(gesture) = self.view.workspace.gesture.take() else {
            return false;
        };
        match gesture {
            Gesture::Resize { before, .. } => {
                self.view.workspace.root = Some(before.clone());
                self.settings.workspace.root = Some(before);
            }
            Gesture::Seek(_) => self.cancel_seek(),
            _ => {}
        }
        self.view.workspace.drop = None;
        true
    }

    pub(crate) fn workspace_key(&mut self, key: KeyPress) -> Result<bool> {
        if key.key == Key::Tab && !key.ctrl && !key.alt {
            self.view.pointer = Position::new(u16::MAX, u16::MAX);
            let panels: Vec<_> = self
                .view
                .workspace
                .areas
                .iter()
                .map(|(panel, _)| *panel)
                .collect();
            if self.view.toolbar_selected.is_some() {
                let next = if key.shift {
                    panels.last()
                } else {
                    panels.first()
                };
                if let Some(panel) = next {
                    self.focus_panel(*panel);
                }
            } else if let Some(index) = panels
                .iter()
                .position(|panel| *panel == self.view.workspace.focus)
            {
                if (key.shift && index == 0) || (!key.shift && index + 1 == panels.len()) {
                    self.focus_toolbar(key.shift);
                } else {
                    let next = if key.shift { index - 1 } else { index + 1 };
                    self.focus_panel(panels[next]);
                }
            } else {
                self.focus_toolbar(key.shift);
            }
            return Ok(true);
        }
        if self.view.toolbar_selected.is_some() {
            return Ok(false);
        }
        if key.ctrl && key.alt && key.key == Key::Char(' ') {
            self.toggle_panel(self.view.workspace.focus)?;
            return Ok(true);
        }
        if key.ctrl && key.alt && key.key == Key::Delete {
            self.close_panel(self.view.workspace.focus)?;
            return Ok(true);
        }
        let edge = match key.key {
            Key::Left => Edge::Left,
            Key::Right => Edge::Right,
            Key::Up => Edge::Top,
            Key::Down => Edge::Bottom,
            _ => return Ok(false),
        };
        if !key.ctrl || (!key.alt && !key.shift) {
            return Ok(false);
        }
        let panel = self.view.workspace.focus;
        let Some((_, area)) = self
            .view
            .workspace
            .areas
            .iter()
            .find(|(value, _)| *value == panel)
            .copied()
        else {
            return Ok(true);
        };
        if key.alt {
            let center = (
                i32::from(area.x + area.width / 2),
                i32::from(area.y + area.height / 2),
            );
            let neighbor = self
                .view
                .workspace
                .areas
                .iter()
                .filter(|(value, rect)| {
                    *value != panel
                        && match edge {
                            Edge::Left => rect.right() <= area.x,
                            Edge::Right => rect.x >= area.right(),
                            Edge::Top => rect.bottom() <= area.y,
                            Edge::Bottom => rect.y >= area.bottom(),
                            Edge::Center => false,
                        }
                })
                .min_by_key(|(_, rect)| {
                    let dx = i32::from(rect.x + rect.width / 2) - center.0;
                    let dy = i32::from(rect.y + rect.height / 2) - center.1;
                    i64::from(dx).pow(2) + i64::from(dy).pow(2)
                })
                .map(|(panel, _)| *panel);
            if let Some(neighbor) = neighbor
                && let Some(mut root) = self.view.workspace.root.clone()
            {
                root.swap(panel, neighbor);
                if crate::ui::workspace_fits(self, &root) {
                    self.settings.workspace.root = Some(root.clone());
                    self.view.workspace.root = Some(root);
                    self.save_settings()?;
                }
            }
        } else {
            let split = self
                .view
                .workspace
                .splits
                .iter()
                .rev()
                .find(|split| split.axis == edge.axis() && split.area.contains(area.as_position()))
                .cloned();
            if let Some(split) = split
                && let Some(mut root) = self.view.workspace.root.clone()
                && let Some(Dock::Split { ratio, .. }) = root.at_mut(&split.path)
            {
                *ratio = if matches!(edge, Edge::Left | Edge::Top) {
                    ratio.saturating_sub(30).max(1)
                } else {
                    ratio.saturating_add(30).min(999)
                };
                if crate::ui::workspace_fits(self, &root) {
                    self.settings.workspace.root = Some(root.clone());
                    self.view.workspace.root = Some(root);
                    self.save_settings()?;
                }
            }
        }
        Ok(true)
    }
}
