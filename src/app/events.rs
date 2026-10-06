use super::{App, Dialog, Focus, Target};
use crate::errors::AppError;
use crate::{
    input::{Input, Key, KeyPress},
    model::Language,
};
use anyhow::{Result, ensure};
use ratatui::layout::{Position, Rect};
use std::time::Duration;

impl App {
    pub fn accepts_text(&self) -> bool {
        matches!(self.view.dialog, Some(Dialog::Text(_)))
    }

    pub fn hovered(&self, area: Rect) -> bool {
        area.contains(self.view.pointer)
    }

    pub fn hovered_target(&self) -> Option<&Target> {
        self.target_at(self.view.pointer)
    }

    pub fn target_at(&self, point: Position) -> Option<&Target> {
        self.view
            .hits
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
                self.view.pointer = Position::new(x, y);
                self.hover_settings();
                if let Some(Target::CommandRow(index)) = self.target_at(self.view.pointer).cloned()
                    && let Some(Dialog::Commands { selected, .. }) = &mut self.view.dialog
                {
                    *selected = index;
                }
            }
            Input::Click { x, y, double } => {
                self.view.pointer = Position::new(x, y);
                if self.view.dialog.is_none() {
                    self.view.toolbar_selected = None;
                    self.focus_point(self.view.pointer);
                }
                let hit = self.target_at(self.view.pointer).cloned();
                if let Some(target) = hit {
                    self.target(target, double)?;
                }
            }
            Input::Drag { x, y } => {
                self.view.pointer = Position::new(x, y);
                self.drag_workspace(self.view.pointer)?;
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
                if matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
                    let position = Position::new(x, y);
                    self.view.settings.focus = if self.view.settings.menu_area.contains(position) {
                        super::SettingsFocus::Menu
                    } else if self.view.settings.subtab_area.contains(position) {
                        super::SettingsFocus::Subtabs
                    } else if self.view.settings.parameters_area.contains(position) {
                        super::SettingsFocus::Parameters
                    } else {
                        return Ok(());
                    };
                    self.view.pointer = Position::new(u16::MAX, u16::MAX);
                }
                if self.view.dialog.is_some() {
                    self.scroll_dialog(delta);
                } else if self.view.playlist_area.contains(Position::new(x, y)) {
                    self.focus_panel(crate::workspace::Panel::Playlists);
                    self.navigate(delta as i64);
                } else if self.view.tracks_area.contains(Position::new(x, y)) {
                    self.focus_panel(crate::workspace::Panel::Tracks);
                    self.navigate(delta as i64);
                }
            }
            Input::Text(text) => {
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
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
                self.view.pointer = Position::new(x, y);
                if self.view.dialog.as_ref().is_some_and(Dialog::is_settings) {
                    let target = self
                        .view
                        .hits
                        .iter()
                        .rev()
                        .find(|hit| hit.enabled && hit.area.contains(self.view.pointer))
                        .map(|hit| hit.target.clone());
                    if let Some(target) = target {
                        self.reverse_target(target)?;
                    }
                } else if self.view.dialog.is_none() {
                    let target = self.target_at(self.view.pointer).cloned();
                    let menu = match target {
                        Some(Target::Playlist(id)) => {
                            self.select_playlist(id);
                            Some(super::CommandMenu::Playlist)
                        }
                        Some(Target::Track(id) | Target::Mark(id)) => {
                            self.library.selected_entry = Some(id);
                            Some(super::CommandMenu::Tracks)
                        }
                        _ if self.view.playlist_area.contains(self.view.pointer) => {
                            Some(super::CommandMenu::Playlist)
                        }
                        _ if self.view.tracks_area.contains(self.view.pointer) => {
                            Some(super::CommandMenu::Tracks)
                        }
                        _ if self.view.workspace.areas.iter().any(|(panel, area)| {
                            *panel == crate::workspace::Panel::Player
                                && area.contains(self.view.pointer)
                        }) =>
                        {
                            Some(super::CommandMenu::Player)
                        }
                        _ => None,
                    };
                    if let Some(menu) = menu {
                        self.open_commands(menu);
                    }
                }
            }
            Input::Key(key) => {
                let was_settings = self.view.dialog.as_ref().is_some_and(Dialog::is_settings);
                let result = self.key(key);
                if was_settings || self.view.dialog.as_ref().is_some_and(Dialog::is_settings) {
                    self.view.pointer = Position::new(u16::MAX, u16::MAX);
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
                if self.view.toolbar_selected.take().is_some() {
                    return Ok(());
                }
                if self.view.layout_editing {
                    self.view.layout_editing = false;
                    return Ok(());
                }
                self.library.query.clear();
                self.refresh()?;
            }
            return Ok(());
        }
        let normalized = key.normalized();
        if let Some(action) = self.view.dialog.as_ref().and_then(Dialog::opening_action)
            && (self
                .settings
                .bindings
                .iter()
                .any(|binding| binding.action == action && binding.key == normalized)
                || (matches!(self.view.dialog, Some(Dialog::SettingsHelp { .. }))
                    && key == KeyPress::plain(Key::F(1))))
        {
            self.close_dialog();
            return Ok(());
        }
        if matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
            return self.settings_key(key);
        }
        if let Some(Dialog::CaptureBinding { index }) = self.view.dialog.as_ref() {
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
                AppError::ReservedShortcut
            );
            ensure!(
                !self
                    .settings
                    .bindings
                    .iter()
                    .enumerate()
                    .any(|(other, binding)| other != index && binding.key == key),
                AppError::ShortcutConflict
            );
            self.settings.bindings[index].key = key;
            self.save_settings()?;
            self.view.settings.page = super::SettingsPage::Shortcuts;
            self.view.settings.selected = 1 + index;
            self.view.settings.focus = super::SettingsFocus::Parameters;
            self.show_settings();
            return Ok(());
        }
        if self.view.dialog.is_some() {
            if matches!(self.view.dialog, Some(Dialog::Commands { .. }))
                && self.view.toolbar_selected.is_some()
                && !key.ctrl
                && !key.alt
            {
                if key.key == Key::Tab {
                    self.close_dialog();
                    self.workspace_key(key)?;
                    return Ok(());
                }
                if matches!(key.key, Key::Left | Key::Right) {
                    self.close_dialog();
                    self.toolbar_key(key)?;
                    if let Some(Target::CommandMenu(menu)) = self
                        .view
                        .toolbar_selected
                        .and_then(|index| self.toolbar_items().into_iter().nth(index))
                        .map(|item| item.target)
                    {
                        self.open_commands(menu);
                    }
                    return Ok(());
                }
            }
            if matches!(self.view.dialog, Some(Dialog::Commands { .. }))
                && let Some(action) = self
                    .settings
                    .bindings
                    .iter()
                    .find(|binding| binding.key == normalized)
                    .map(|binding| binding.action)
            {
                self.view.dialog = None;
                return self.action(action);
            }
            if !key.ctrl
                && !key.alt
                && let Some(Dialog::Panels { selected, .. }) = &self.view.dialog
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
            if matches!(self.view.dialog, Some(Dialog::Browser(_))) {
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
                && let Some(Dialog::Text(dialog)) = &mut self.view.dialog
            {
                dialog.selected_all = true;
                return Ok(());
            }
            match key.key {
                Key::Enter => self.submit()?,
                Key::Backspace => {
                    if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
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
                    if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
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
        if self.toolbar_key(key)? {
            return Ok(());
        }
        if matches!(self.view.workspace.focus, crate::workspace::Panel::Player)
            && !key.ctrl
            && !key.alt
            && !key.shift
            && matches!(key.key, Key::Up | Key::Down)
        {
            self.adjust_volume(if key.key == Key::Up { 1 } else { -1 })?;
            return Ok(());
        }
        if key.key == Key::F(10) && !key.ctrl && !key.alt {
            self.view.pointer = Position::new(u16::MAX, u16::MAX);
            self.open_commands(
                if self.view.workspace.focus == crate::workspace::Panel::Player {
                    super::CommandMenu::Player
                } else if self.view.focus == Focus::Playlists {
                    super::CommandMenu::Playlist
                } else {
                    super::CommandMenu::Tracks
                },
            );
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
                if self.view.focus == Focus::Playlists {
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
            Target::CommandMenu(menu) => self.open_commands(menu),
            Target::CommandRow(index) => self.activate_command(index)?,
            Target::ManagePanels => {
                self.view.dialog = Some(Dialog::Panels {
                    selected: 0,
                    expanded: Vec::new(),
                });
            }
            Target::PanelFocus(panel) => self.focus_panel(panel),
            Target::PanelMove(panel) => {
                self.focus_panel(panel);
                self.view.workspace.gesture = Some(crate::workspace::Gesture::Move {
                    panel,
                    origin: self.view.pointer,
                });
            }
            Target::PanelCollapse(panel) => self.toggle_panel(panel)?,
            Target::PanelClose(panel) => self.close_panel(panel)?,
            Target::PanelVisibility(panel) => self.show_panel(panel)?,
            Target::PanelRow(index) => self.activate_panel_row(index, false)?,
            Target::PanelResize(index) => {
                if let (Some(split), Some(root)) = (
                    self.view.workspace.splits.get(index),
                    &self.view.workspace.root,
                ) {
                    self.view.workspace.gesture = Some(crate::workspace::Gesture::Resize {
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
                self.view.focus = Focus::Playlists;
            }
            Target::Track(id) => {
                self.library.selected_entry = Some(id);
                self.view.focus = Focus::Tracks;
                if double {
                    self.play_selected()?;
                }
            }
            Target::Mark(id) => {
                self.library.selected_entry = Some(id);
                self.view.focus = Focus::Tracks;
                if !self.library.marked.insert(id) {
                    self.library.marked.remove(&id);
                }
            }
            Target::SortColumn(sort) => {
                self.library.sort_descending =
                    self.library.sort == sort && !self.library.sort_descending;
                self.library.sort = sort;
                self.view.track_offset = 0;
                self.view.focus = Focus::Tracks;
                self.refresh()?;
            }
            Target::PlaybackVolume(area) => {
                if self.view.workspace.gesture.is_none() {
                    self.view.workspace.gesture =
                        Some(crate::workspace::Gesture::PlaybackVolume(area));
                }
                let percent = (u32::from(self.last_pointer_x(area)) * 100
                    / u32::from(area.width.saturating_sub(1).max(1)))
                    as i16;
                self.adjust_volume(percent - (self.settings.volume * 100.0).round() as i16)?;
            }
            Target::Seek(area) => {
                if self.view.workspace.gesture.is_none() {
                    self.view.workspace.gesture = Some(crate::workspace::Gesture::Seek(area));
                }
                if let (Some(audio), Some(current)) = (&self.playback.audio, &self.playback.current)
                {
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
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
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
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    } else {
                        dialog.text.pop();
                    }
                }
            }
            Target::KeyboardLanguage => {
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    dialog.keyboard = if dialog.keyboard == Language::Russian {
                        Language::English
                    } else {
                        Language::Russian
                    };
                }
            }
            Target::KeyboardCase => {
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    dialog.upper = !dialog.upper;
                }
            }
            Target::BrowserRow(index) => {
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
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
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
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
                if let Some(Dialog::Transfer { selected, .. }) = &mut self.view.dialog {
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
                    if self.view.workspace.gesture.is_none() {
                        self.view.workspace.gesture =
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
                if matches!(self.view.dialog, Some(Dialog::CaptureBinding { .. }))
                    && let Some(value) = self.view.settings.binding_modifiers.get_mut(index)
                {
                    *value = !*value;
                }
            }
            Target::BindingKey(key) => {
                if matches!(self.view.dialog, Some(Dialog::CaptureBinding { .. })) {
                    let [ctrl, alt, shift] = self.view.settings.binding_modifiers;
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
                self.view.pointer = Position::new(u16::MAX, u16::MAX);
                self.view.settings.focus = super::SettingsFocus::Menu;
                self.settings_scroll(-1);
            }
            Target::SettingsTab(_) => {
                self.view.pointer = Position::new(u16::MAX, u16::MAX);
                self.view.settings.focus = super::SettingsFocus::Subtabs;
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
        self.view.pointer.x.saturating_sub(area.x).min(area.width)
    }
}
