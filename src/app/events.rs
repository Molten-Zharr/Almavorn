use super::{App, Dialog, Focus, Target};
use crate::{
    input::{Input, Key, KeyPress},
    model::Language,
};
use anyhow::{Result, ensure};
use ratatui::layout::{Position, Rect};
use std::time::Duration;

impl App {
    pub fn accepts_text(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Text(_)))
    }

    pub fn hovered(&self, area: Rect) -> bool {
        area.contains(self.pointer)
    }

    pub fn target_at(&self, point: Position) -> Option<&Target> {
        self.hits
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.enabled && hit.area.contains(point))
            .max_by_key(|(index, hit)| {
                (
                    matches!(
                        hit.target,
                        Target::PanelMove(_) | Target::PanelCollapse(_) | Target::PanelClose(_)
                    ),
                    *index,
                )
            })
            .map(|(_, hit)| &hit.target)
    }

    pub fn handle(&mut self, input: Input) {
        if let Err(error) = self.handle_inner(input) {
            self.failure(error);
        }
    }

    fn handle_inner(&mut self, input: Input) -> Result<()> {
        match input {
            Input::Move { x, y } => {
                self.pointer = Position::new(x, y);
                self.hover_settings();
            }
            Input::Click { x, y, double } => {
                self.pointer = Position::new(x, y);
                if self.dialog.is_none() {
                    self.focus_point(self.pointer);
                }
                let hit = self.target_at(self.pointer).cloned();
                if let Some(target) = hit {
                    self.target(target, double)?;
                }
            }
            Input::Drag { x, y } => {
                self.pointer = Position::new(x, y);
                self.drag_workspace(self.pointer)?;
            }
            Input::Release { x, y } => self.release_workspace(Position::new(x, y))?,
            Input::CancelPointer => {
                self.cancel_workspace_drag();
            }
            Input::Scroll { x, y, delta } => {
                let delta = delta.signum();
                if delta == 0 {
                    return Ok(());
                }
                if matches!(self.dialog, Some(Dialog::Settings { .. })) {
                    let position = Position::new(x, y);
                    self.settings_view.focus = if self.settings_view.menu_area.contains(position) {
                        super::SettingsFocus::Menu
                    } else if self.settings_view.subtab_area.contains(position) {
                        super::SettingsFocus::Subtabs
                    } else if self.settings_view.parameters_area.contains(position) {
                        super::SettingsFocus::Parameters
                    } else {
                        return Ok(());
                    };
                    self.pointer = Position::new(u16::MAX, u16::MAX);
                }
                if self.dialog.is_some() {
                    self.scroll_dialog(delta);
                } else if self.playlist_area.contains(Position::new(x, y)) {
                    self.focus = Focus::Playlists;
                    self.navigate(delta as i64);
                } else if self.tracks_area.contains(Position::new(x, y)) {
                    self.focus = Focus::Tracks;
                    self.navigate(delta as i64);
                }
            }
            Input::Text(text) => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    let remaining = 512usize.saturating_sub(dialog.text.chars().count());
                    dialog
                        .text
                        .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                }
            }
            Input::SecondaryClick { x, y } => {
                self.pointer = Position::new(x, y);
                if self.dialog.as_ref().is_some_and(Dialog::is_settings) {
                    let target = self
                        .hits
                        .iter()
                        .rev()
                        .find(|hit| hit.enabled && hit.area.contains(self.pointer))
                        .map(|hit| hit.target.clone());
                    if let Some(target) = target {
                        self.reverse_target(target)?;
                    }
                }
            }
            Input::Key(key) => {
                let was_settings = self.dialog.as_ref().is_some_and(Dialog::is_settings);
                let result = self.key(key);
                if was_settings || self.dialog.as_ref().is_some_and(Dialog::is_settings) {
                    self.pointer = Position::new(u16::MAX, u16::MAX);
                }
                result?;
            }
        }
        Ok(())
    }

    fn key(&mut self, key: KeyPress) -> Result<()> {
        if key.key == Key::Escape {
            if self.cancel_workspace_drag() {
                return Ok(());
            }
            if !self.close_dialog() {
                self.query.clear();
                self.refresh()?;
            }
            return Ok(());
        }
        let normalized = key.normalized();
        if let Some(action) = self.dialog.as_ref().and_then(Dialog::opening_action)
            && (self
                .settings
                .bindings
                .iter()
                .any(|binding| binding.action == action && binding.key == normalized)
                || (matches!(self.dialog, Some(Dialog::SettingsHelp { .. }))
                    && key == KeyPress::plain(Key::F(1))))
        {
            self.close_dialog();
            return Ok(());
        }
        if matches!(self.dialog, Some(Dialog::Settings { .. })) {
            return self.settings_key(key);
        }
        if let Some(Dialog::CaptureBinding { index }) = self.dialog.as_ref() {
            let index = *index;
            let key = key.normalized();
            ensure!(
                !matches!(
                    key.key,
                    Key::Tab
                        | Key::Up
                        | Key::Down
                        | Key::Left
                        | Key::Right
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown
                ) || key.ctrl
                    || key.alt,
                "Navigation keys are reserved"
            );
            ensure!(
                !self
                    .settings
                    .bindings
                    .iter()
                    .enumerate()
                    .any(|(other, binding)| other != index && binding.key == key),
                "This shortcut is already assigned"
            );
            self.settings.bindings[index].key = key;
            self.save_settings()?;
            self.settings_view.page = super::SettingsPage::Shortcuts;
            self.settings_view.selected = 1 + index;
            self.settings_view.focus = super::SettingsFocus::Parameters;
            self.show_settings();
            return Ok(());
        }
        if self.dialog.is_some() {
            if !key.ctrl
                && !key.alt
                && let Some(Dialog::Panels { selected, .. }) = &self.dialog
            {
                let selected = *selected;
                match key.key {
                    Key::Left | Key::Right => {
                        self.expand_panel_category(key.key == Key::Right);
                        return Ok(());
                    }
                    Key::Char(' ') => {
                        self.activate_panel_row(selected, true)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if matches!(self.dialog, Some(Dialog::Browser(_))) {
                if key.key == Key::Char('a') && key.ctrl {
                    return self.target(Target::BrowserMarkAll, false);
                }
                if (key.key == Key::Enter && key.ctrl) || key.key == Key::Char('a') {
                    return self.browser_add();
                }
                if key.key == Key::Backspace {
                    return self.target(Target::BrowserParent, false);
                }
            }
            if key.ctrl
                && key.key == Key::Char('a')
                && let Some(Dialog::Text(dialog)) = &mut self.dialog
            {
                dialog.selected_all = true;
                return Ok(());
            }
            match key.key {
                Key::Enter => self.submit()?,
                Key::Backspace => {
                    if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                        if dialog.selected_all {
                            dialog.text.clear();
                            dialog.selected_all = false;
                        } else {
                            dialog.text.pop();
                        }
                    }
                }
                Key::Up => self.scroll_dialog(-1),
                Key::Down => self.scroll_dialog(1),
                Key::PageUp => self.scroll_dialog(-8),
                Key::PageDown => self.scroll_dialog(8),
                Key::Char(' ') => {
                    if let Some(Dialog::Browser(browser)) = &mut self.dialog
                        && let Some(entry) = browser.entries.get(browser.selected)
                        && !entry.directory
                        && !browser.marked.insert(entry.path.clone())
                    {
                        browser.marked.remove(&entry.path);
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if matches!(self.workspace.focus, crate::workspace::Panel::Player)
            && !key.ctrl
            && !key.alt
            && !key.shift
            && matches!(key.key, Key::Up | Key::Down)
        {
            self.adjust_volume(if key.key == Key::Up { 1 } else { -1 })?;
            return Ok(());
        }
        if let Some(action) = self
            .settings
            .bindings
            .iter()
            .find(|binding| binding.key == normalized)
            .map(|binding| binding.action)
        {
            if self.workspace_key(key)? {
                return Ok(());
            }
            self.action(action)?;
            return Ok(());
        }
        if self.workspace_key(key)? {
            return Ok(());
        }
        match key.key {
            Key::Up => self.navigate(-1),
            Key::Down => self.navigate(1),
            Key::PageUp => self.navigate(-10),
            Key::PageDown => self.navigate(10),
            Key::Home => self.navigate(i64::MIN),
            Key::End => self.navigate(i64::MAX),
            Key::Enter => {
                if self.focus == Focus::Playlists {
                    self.focus_panel(crate::workspace::Panel::Tracks);
                } else {
                    self.play_selected()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn target(&mut self, target: Target, double: bool) -> Result<()> {
        match target {
            Target::PanelFocus(panel) => self.focus_panel(panel),
            Target::PanelMove(panel) => {
                self.focus_panel(panel);
                self.workspace.gesture = Some(crate::workspace::Gesture::Move {
                    panel,
                    origin: self.pointer,
                });
            }
            Target::PanelCollapse(panel) => self.toggle_panel(panel)?,
            Target::PanelClose(panel) => self.close_panel(panel)?,
            Target::PanelVisibility(panel) => self.show_panel(panel)?,
            Target::PanelRow(index) => self.activate_panel_row(index, false)?,
            Target::PanelResize(index) => {
                if let (Some(split), Some(root)) =
                    (self.workspace.splits.get(index), &self.workspace.root)
                {
                    self.workspace.gesture = Some(crate::workspace::Gesture::Resize {
                        split: split.clone(),
                        before: root.clone(),
                    });
                }
            }
            Target::ResetWorkspace => self.reset_workspace()?,
            Target::Action(action) => self.action(action)?,
            Target::Mode(mode) => self.set_mode(mode)?,
            Target::Playlist(id) => {
                self.select_playlist(id);
                self.focus = Focus::Playlists;
            }
            Target::Track(id) => {
                self.selected_entry = Some(id);
                self.focus = Focus::Tracks;
                if double {
                    self.play_selected()?;
                }
            }
            Target::Mark(id) => {
                self.selected_entry = Some(id);
                self.focus = Focus::Tracks;
                if !self.marked.insert(id) {
                    self.marked.remove(&id);
                }
            }
            Target::Seek(area) => {
                if self.workspace.gesture.is_none() {
                    self.workspace.gesture = Some(crate::workspace::Gesture::Seek(area));
                }
                if let (Some(audio), Some(current)) = (&self.audio, &self.current) {
                    let duration = current.duration_ms;
                    if duration > 0 && area.width > 0 {
                        let point = self.last_pointer_x(area);
                        audio.seek(Duration::from_millis(
                            duration.saturating_mul(point as u64)
                                / area.width.saturating_sub(1).max(1) as u64,
                        ))?;
                    }
                }
            }
            Target::CloseDialog => {
                self.close_dialog();
            }
            Target::Submit => self.submit()?,
            Target::Text(c) => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    if dialog.text.chars().count() < 512 {
                        dialog.text.push(c);
                    }
                }
            }
            Target::Backspace => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    } else {
                        dialog.text.pop();
                    }
                }
            }
            Target::KeyboardLanguage => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    dialog.keyboard = if dialog.keyboard == Language::Russian {
                        Language::English
                    } else {
                        Language::Russian
                    };
                }
            }
            Target::KeyboardCase => {
                if let Some(Dialog::Text(dialog)) = &mut self.dialog {
                    dialog.upper = !dialog.upper;
                }
            }
            Target::BrowserRow(index) => {
                if let Some(Dialog::Browser(browser)) = &mut self.dialog {
                    browser.selected = index;
                    if let Some(entry) = browser.entries.get(index)
                        && !entry.directory
                        && !browser.marked.insert(entry.path.clone())
                    {
                        browser.marked.remove(&entry.path);
                    }
                }
                if double {
                    self.browser_open()?;
                }
            }
            Target::BrowserParent => {
                self.browser_parent();
            }
            Target::BrowserMarkAll => {
                if let Some(Dialog::Browser(browser)) = &mut self.dialog {
                    for entry in &browser.entries {
                        if !entry.directory {
                            browser.marked.insert(entry.path.clone());
                        }
                    }
                }
            }
            Target::BrowserOpen => self.browser_open()?,
            Target::BrowserAdd => self.browser_add()?,
            Target::TransferRow(index) => {
                if let Some(Dialog::Transfer { selected, .. }) = &mut self.dialog {
                    *selected = index;
                }
                if double {
                    self.submit()?;
                }
            }
            Target::DialogScroll(delta) => self.scroll_dialog(delta),
            Target::Setting(index) => self.setting(index)?,
            Target::SettingSelect(index) => {
                self.select_setting(index);
                if double {
                    self.settings_help(index);
                } else {
                    self.adjust_setting(index, 1)?;
                }
            }
            Target::SettingAdjust(index, direction) => self.adjust_setting(index, direction)?,
            Target::SettingsPage(page) => self.open_settings_page(page),
            Target::SettingsTab(page) => self.select_settings_tab(page),
            Target::SettingsVolume(index, area) => {
                if area.width > 0 {
                    if self.workspace.gesture.is_none() {
                        self.workspace.gesture =
                            Some(crate::workspace::Gesture::Volume(index, area));
                    }
                    self.select_setting(index);
                    let point = self.last_pointer_x(area);
                    self.set_settings_volume(
                        f32::from(point) / f32::from(area.width.saturating_sub(1).max(1)),
                    )?;
                }
            }
            Target::SettingHelp(index) => self.settings_help(index),
            Target::BindingModifier(index) => {
                if matches!(self.dialog, Some(Dialog::CaptureBinding { .. }))
                    && let Some(value) = self.settings_view.binding_modifiers.get_mut(index)
                {
                    *value = !*value;
                }
            }
            Target::BindingKey(key) => {
                if matches!(self.dialog, Some(Dialog::CaptureBinding { .. })) {
                    let [ctrl, alt, shift] = self.settings_view.binding_modifiers;
                    self.key(KeyPress {
                        key,
                        ctrl,
                        alt,
                        shift,
                    })?;
                }
            }
        }
        Ok(())
    }

    fn reverse_target(&mut self, target: Target) -> Result<()> {
        match target {
            Target::Setting(index)
            | Target::SettingSelect(index)
            | Target::SettingsVolume(index, _) => self.adjust_setting(index, -1)?,
            Target::SettingAdjust(index, direction) => self.adjust_setting(index, -direction)?,
            Target::SettingsPage(_) => {
                self.pointer = Position::new(u16::MAX, u16::MAX);
                self.settings_view.focus = super::SettingsFocus::Menu;
                self.settings_scroll(-1);
            }
            Target::SettingsTab(_) => {
                self.pointer = Position::new(u16::MAX, u16::MAX);
                self.settings_view.focus = super::SettingsFocus::Subtabs;
                self.settings_scroll(-1);
            }
            Target::DialogScroll(delta) => self.scroll_dialog(-delta),
            Target::CloseDialog => {
                self.close_dialog();
            }
            _ => {}
        }
        Ok(())
    }

    fn last_pointer_x(&self, area: Rect) -> u16 {
        self.pointer.x.saturating_sub(area.x).min(area.width)
    }
}
