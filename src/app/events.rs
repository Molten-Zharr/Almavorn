use super::{App, Dialog, Focus, Target};
use crate::errors::AppError;
use crate::{
    input::{Input, Key, KeyPress},
    model::Language,
};
use anyhow::{Result, ensure};
use ratatui::layout::{Position, Rect};

impl App {
    pub fn accepts_text(&self) -> bool {
        matches!(self.view.dialog, Some(Dialog::Text(_)))
            || matches!(self.view.dialog, Some(Dialog::Search(_)))
            || (self.view.dialog.is_none() && self.view.filter_editing)
    }

    pub fn hovered(&self, area: Rect) -> bool {
        area.contains(self.view.pointer)
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
            }
            Input::Click { x, y, double } => {
                self.view.pointer = Position::new(x, y);
                if self.view.dialog.is_none() {
                    self.focus_point(self.view.pointer);
                }
                let hit = self.target_at(self.view.pointer).cloned();
                if self.view.dialog.is_none()
                    && self.view.filter_editing
                    && !matches!(
                        hit,
                        Some(
                            Target::Action(crate::input::Action::Filter)
                                | Target::FilterInput
                                | Target::QueryKeyboard
                                | Target::Text(_)
                                | Target::Backspace
                                | Target::KeyboardLanguage
                                | Target::KeyboardCase
                        )
                    )
                {
                    self.view.filter_editing = false;
                    self.view.filter_keyboard = false;
                }
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
                    if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                        search.focus = if search.playlists_area.contains(Position::new(x, y)) {
                            super::SearchFocus::Playlists
                        } else {
                            super::SearchFocus::Results
                        };
                    }
                    self.scroll_dialog(delta);
                } else if matches!(
                    self.target_at(Position::new(x, y)),
                    Some(Target::PlaybackVolume(_))
                ) {
                    self.adjust_volume(-delta)?;
                } else if self.view.playlist_area.contains(Position::new(x, y)) {
                    self.view.focus = Focus::Playlists;
                    self.navigate(delta as i64);
                } else if self.view.tracks_area.contains(Position::new(x, y)) {
                    self.view.focus = Focus::Tracks;
                    self.navigate(delta as i64);
                }
            }
            Input::Text(text) => {
                if self.query_text(&text)? {
                    return self.tick_search();
                }
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
        self.tick_search()
    }

    fn key(&mut self, key: KeyPress) -> Result<()> {
        if self.view.dialog.is_none() && self.view.filter_editing {
            return self.filter_key(key);
        }
        if key.key == Key::Escape {
            if self.cancel_workspace_drag() {
                return Ok(());
            }
            if !self.close_dialog() {
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
            if matches!(self.view.dialog, Some(Dialog::Search(_))) {
                return self.search_key(key);
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
        if matches!(self.view.workspace.focus, crate::workspace::Panel::Player)
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
            Target::FilterInput => {
                if !self.view.filter_editing {
                    self.edit_filter()?;
                }
            }
            Target::ClearFilter => {
                self.library.query.clear();
                self.view.filter_editing = false;
                self.view.filter_keyboard = false;
                self.view.track_offset = 0;
                self.refresh()?;
            }
            Target::SearchInput => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.focus = super::SearchFocus::Input;
                }
            }
            Target::SearchResult(index) => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.selected = index.min(search.results.len().saturating_sub(1));
                    search.focus = super::SearchFocus::Results;
                }
                if double {
                    self.play_search_result()?;
                }
            }
            Target::SearchPlaylist(id) => self.toggle_search_playlist(id),
            Target::SearchPlay => self.play_search_result()?,
            Target::SearchAll(all) => {
                let ids = if all {
                    self.playlists().iter().map(|p| p.id).collect()
                } else {
                    Default::default()
                };
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.playlists = ids;
                }
            }
            Target::QueryKeyboard => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.keyboard_visible = !search.keyboard_visible;
                    search.focus = super::SearchFocus::Input;
                } else {
                    if !self.view.filter_editing {
                        self.edit_filter()?;
                    }
                    self.view.filter_keyboard = !self.view.filter_keyboard;
                }
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
                if let Some(current) = &self.playback.current {
                    let duration = current.duration_ms;
                    if duration > 0 && area.width > 0 {
                        let point = self.last_pointer_x(area);
                        self.preview_seek(
                            duration.saturating_mul(point as u64)
                                / area.width.saturating_sub(1).max(1) as u64,
                        );
                    }
                }
            }
            Target::CloseDialog => {
                self.close_dialog();
            }
            Target::Submit => self.submit()?,
            Target::Text(c) => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.focus = super::SearchFocus::Input;
                }
                if self.query_text(&c.to_string())? {
                    return Ok(());
                }
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
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.focus = super::SearchFocus::Input;
                    self.search_key(KeyPress::plain(Key::Backspace))?;
                    return Ok(());
                }
                if self.view.dialog.is_none() && self.view.filter_editing {
                    return self.filter_key(KeyPress::plain(Key::Backspace));
                }
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
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.keyboard = if search.keyboard == Language::Russian {
                        Language::English
                    } else {
                        Language::Russian
                    };
                    return Ok(());
                }
                if self.view.dialog.is_none() && self.view.filter_editing {
                    self.view.filter_keyboard_language =
                        if self.view.filter_keyboard_language == Language::Russian {
                            Language::English
                        } else {
                            Language::Russian
                        };
                    return Ok(());
                }
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    dialog.keyboard = if dialog.keyboard == Language::Russian {
                        Language::English
                    } else {
                        Language::Russian
                    };
                }
            }
            Target::KeyboardCase => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.upper = !search.upper;
                    return Ok(());
                }
                if self.view.dialog.is_none() && self.view.filter_editing {
                    self.view.filter_keyboard_upper = !self.view.filter_keyboard_upper;
                    return Ok(());
                }
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
