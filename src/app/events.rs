use super::{App, Dialog, Focus, Target};
use crate::errors::AppError;
use crate::{
    input::{Action, Input, Key, KeyPress},
    model::{Language, RepeatMode},
};
use anyhow::{Result, ensure};
use ratatui::layout::{Position, Rect};

impl App {
    pub fn copy_playing_track_at(&mut self, x: u16, y: u16) -> bool {
        if self.view.dialog.as_ref().is_some_and(|dialog| !matches!(dialog, Dialog::Statistics))
            || !matches!(self.target_at(Position::new(x, y)), Some(Target::PlayingTrack))
        {
            return false;
        }
        let Some(track) = self.playback.current.as_ref() else { return false; };
        let text = [track.artist.trim(), track.title.trim()]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join(" - ");
        if text.is_empty() {
            self.message(self.text("Artist and title are missing", "Исполнитель и название не указаны").into());
        } else {
            self.view.clipboard_text = Some(text);
        }
        true
    }

    pub fn accepts_text(&self) -> bool {
        matches!(self.view.dialog, Some(Dialog::Help { .. }))
            || matches!(&self.view.dialog,Some(Dialog::Form(form)) if !form.discard)
            || matches!(&self.view.dialog, Some(Dialog::Tags(editor)) if editor.accepts_text() && !self.writing_tags())
            || matches!(self.view.dialog, Some(Dialog::Text(_)))
            || matches!(self.view.dialog, Some(Dialog::Search(_)))
            || matches!(self.view.dialog, Some(Dialog::Browser(_)))
            || ((self.view.dialog.is_none()
                || matches!(self.view.dialog, Some(Dialog::Statistics))
                    && !self.statistics.view.focused)
                && self.view.filter_editing)
    }

    pub fn hovered(&self, area: Rect) -> bool {
        area.contains(self.view.pointer)
    }

    pub fn hovered_target(&self) -> Option<&Target> {
        self.target_at(self.view.pointer)
    }

    pub fn target_at(&self, point: Position) -> Option<&Target> {
        if self.view.dialog.is_some()
            && let Some(dropdown) = &self.view.dropdown
            && dropdown.area.contains(point)
        {
            let y = f32::from(point.y) + self.view.pointer_fraction_y;
            return dropdown.rows.iter().find(|row| y >= row.top && y < row.top + row.height)
                .map(|row| &row.target).or(Some(&Target::MenuSection));
        }
        self.view
            .hits
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.enabled && hit.area.contains(point))
            .filter(|(_, hit)| {
                !matches!(self.view.dialog, Some(Dialog::Statistics))
                    || (self.statistics.view.menu.is_none() && self.statistics.view.info.is_none())
                    || matches!(hit.target, Target::StatisticsControl(_))
            })
            .filter(|(_, hit)| {
                !matches!(self.view.dialog, Some(Dialog::Statistics))
                    || !self.view.overlay_area.contains(point)
                    || matches!(
                        hit.target,
                        Target::StatisticsPage(_)
                            | Target::StatisticsSort
                            | Target::StatisticsRow(_)
                            | Target::StatisticsRefresh
                            | Target::StatisticsControl(_)
                            | Target::CloseDialog
                    )
            })
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
        let statistics = if self.statistics_background_input(&input) {
            self.view.dialog.take()
        } else {
            None
        };
        let result = self.handle_inner(input);
        if self.view.dialog.is_none() {
            self.view.dialog = statistics;
        }
        if let Err(error) = result {
            self.failure(error);
        }
    }

    /// Apply GUI volume drags without reducing the pointer to a terminal cell.
    pub fn drag_volume(&mut self, target: Target, ratio: f64, release: bool) {
        use crate::workspace::{Gesture, Panel};
        if !ratio.is_finite() {
            return;
        }
        let gesture = match target {
            Target::PlaybackVolume(area)
                if area.width > 0
                    && (self.view.dialog.is_none()
                        || matches!(self.view.dialog, Some(Dialog::Statistics))) =>
            {
                self.statistics.view.focused = false;
                if self.view.workspace.gesture.is_none() {
                    self.view.toolbar_selected = None;
                    self.view.filter_editing = false;
                    self.view.filter_keyboard = false;
                    self.focus_panel(Panel::Player);
                }
                Gesture::PlaybackVolume(area)
            }
            Target::SettingsVolume(index, area)
                if area.width > 0 && matches!(self.view.dialog, Some(Dialog::Settings { .. })) =>
            {
                self.select_setting(index);
                Gesture::Volume(index, area)
            }
            _ => return,
        };
        if self.view.workspace.gesture.is_none() {
            self.view.workspace.gesture = Some(gesture);
        }
        let percent = (ratio.clamp(0.0, 1.0) * 100.0).round() as i16;
        if let Err(error) =
            self.adjust_volume(percent - (self.settings.preferences.volume * 100.0).round() as i16)
        {
            self.failure(error);
        }
        if release {
            self.view.workspace.gesture = None;
        }
    }

    fn handle_inner(&mut self, input: Input) -> Result<()> {
        match input {
            Input::DroppedFiles { paths, playlist } => {
                self.start_dropped_import(paths, playlist)?;
            }
            Input::Move { x, y } => {
                self.view.pointer = Position::new(x, y);
                self.hover_settings();
                if let Some(Target::StatisticsControl(super::StatisticsControl::Item(index))) = self.target_at(self.view.pointer)
                    && self.statistics.view.menu.is_some()
                {
                    self.statistics.view.menu_selected = *index;
                }
                if let Some(Target::CommandRow(index)) = self.target_at(self.view.pointer).cloned()
                    && let Some(Dialog::Commands { selected, .. }) = &mut self.view.dialog
                {
                    *selected = index;
                }
                if let Some(Target::EqualizerPreset(index)) =
                    self.target_at(self.view.pointer).cloned()
                    && let Some(Dialog::Equalizer {
                        preset_selected, ..
                    }) = &mut self.view.dialog
                {
                    *preset_selected = index;
                }
            }
            Input::Click { x, y, double } => {
                self.finish_browser_mark_drag();
                self.view.mark_drag=None;
                self.view.pointer = Position::new(x, y);
                if self.view.dialog.is_none() {
                    self.lyrics.panel.focused = self.lyrics.panel.area.contains(self.view.pointer);
                }
                if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root)
                    && self.view.settings.parameters_area.contains(self.view.pointer)
                {
                    self.view.settings.focus = super::SettingsFocus::Parameters;
                }
                if self.view.dialog.is_none() {
                    self.view.toolbar_selected = None;
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
                    if matches!(target, Target::Action(Action::CycleRepeat)) {
                        let repeat = if double {
                            RepeatMode::Playlist
                        } else if self.settings.preferences.repeat == RepeatMode::Track {
                            RepeatMode::Off
                        } else {
                            RepeatMode::Track
                        };
                        self.set_repeat(repeat)?;
                    } else {
                        self.target(target, double)?;
                    }
                }
            }
            Input::Drag { x, y } => {
                self.view.pointer = Position::new(x, y);
                if !self.drag_browser_marks(self.view.pointer) && !self.drag_track_marks(self.view.pointer)? {
                    self.drag_workspace(self.view.pointer)?;
                }
            }
            Input::Release { x, y } => {
                self.drag_browser_marks(Position::new(x, y));
                self.finish_browser_mark_drag();
                self.drag_track_marks(Position::new(x,y))?;
                self.view.mark_drag=None;
                self.release_workspace(Position::new(x, y))?;
            },
            Input::CancelPointer => {
                self.cancel_workspace_drag();
            }
            Input::Scroll {
                x, y, delta, ctrl, shift,
            } => {
                let delta = delta.signum();
                if delta == 0 {
                    return Ok(());
                }
                let position = Position::new(x, y);
                if matches!(self.view.dialog, Some(Dialog::Browser(_))) {
                    self.scroll_browser_pages(position, delta, ctrl && !shift);
                    return Ok(());
                }
                if matches!(self.view.dialog, Some(Dialog::Help { .. }))
                    && matches!(self.target_at(position), Some(Target::HelpSection(_)))
                {
                    let step = if ctrl && shift { self.view.dialog_page_size.max(1) as i64 }
                        else if ctrl { 5 } else { 1 };
                    self.move_help_section(i64::from(delta) * step);
                    return Ok(());
                }
                if self.view.dialog.is_none() && self.lyrics.panel.area.contains(position) {
                    self.focus_panel(crate::workspace::Panel::Lyrics);
                    let step = if ctrl && shift { self.lyrics.panel.page_size.max(1) as i64 }
                        else if ctrl { 15 } else { 3 };
                    self.scroll_lyrics_panel(i64::from(delta) * step);
                    return Ok(());
                }
                if self.color_picker_scroll(position, delta) {
                    return Ok(());
                }
                if matches!(self.view.dialog, Some(Dialog::Equalizer { .. })) {
                    let step = self.wheel_step(position, ctrl, shift, 1);
                    return self.scroll_equalizer(position, delta * step);
                }
                if matches!(self.view.dialog, Some(Dialog::PlaybackTuner { .. })) {
                    return self.scroll_tuner(position, delta);
                }
                if matches!(self.view.dialog, Some(Dialog::Statistics)) {
                    if self.statistics.view.menu.is_some() || self.statistics.view.info.is_some() {
                        if self.statistics.view.popup_area.contains(position) {
                            let step = if ctrl && shift { self.statistics.view.popup_area.height.saturating_sub(4).max(1) as i64 }
                                else if ctrl { 5 } else { 1 };
                            self.move_statistics(i64::from(delta) * step);
                        }
                        return Ok(());
                    }
                    let step = self.wheel_step(position, ctrl, shift, 3);
                    let distance = i64::from(delta) * i64::from(step);
                    if ctrl && shift {
                        self.statistics.view.offset = self.statistics.view.offset
                            .saturating_add_signed(distance as isize);
                    }
                    self.move_statistics(distance);
                    return Ok(());
                }
                if self.view.dialog.is_none()
                    && matches!(self.target_at(position), Some(Target::PlaybackVolume(_)))
                {
                    self.adjust_volume(-delta)?;
                    return Ok(());
                }
                if ctrl && self.view.dialog.is_none() {
                    use crate::workspace::Panel;
                    let panel = if self.view.playlist_area.contains(position) {
                        Some(Panel::Playlists)
                    } else if self.view.tracks_area.contains(position) {
                        Some(Panel::Tracks)
                    } else {
                        None
                    };
                    let scroll = self.view.hits.iter().find_map(|hit| match hit.target {
                        Target::ListScroll(scroll)
                            if Some(scroll.panel) == panel || hit.area.contains(position) => Some(scroll),
                        _ => None,
                    });
                    if let Some(scroll) = scroll {
                        let step = if shift { scroll.visible.max(1) } else { 5 };
                        self.scroll_list(scroll, i64::from(delta) * step as i64);
                        if matches!(
                            self.view.workspace.gesture,
                            Some(crate::workspace::Gesture::Playlist { .. })
                        ) {
                            self.drag_workspace(position)?;
                        }
                        return Ok(());
                    }
                }
                if matches!(self.view.dialog, Some(Dialog::CaptureBinding { .. })) {
                    let step = self.wheel_step(position, ctrl, shift, 8);
                    self.view.settings.binding_offset = super::bounded(
                        self.view.settings.binding_offset,
                        i64::from(delta) * i64::from(step),
                        Key::shortcut_choices().count(),
                    );
                    return Ok(());
                }
                let delta = delta * self.wheel_step(position, ctrl, shift, 1);
                if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root) {
                    let position = Position::new(x, y);
                    self.view.settings.focus = if self.view.settings.menu_area.contains(position) {
                        super::SettingsFocus::Menu
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
                    if ctrl && shift {
                        self.shift_dialog_page(delta);
                    }
                    self.scroll_dialog(delta);
                } else if let Some(Target::ListScroll(scroll)) =
                    self.target_at(Position::new(x, y)).cloned()
                {
                    self.scroll_list(scroll, i64::from(delta));
                } else if self.view.playlist_area.contains(Position::new(x, y)) {
                    if matches!(
                        self.view.workspace.gesture,
                        Some(crate::workspace::Gesture::Playlist { .. })
                    ) {
                        let maximum = self
                            .visible_playlists()
                            .len()
                            .saturating_sub(usize::from(self.view.playlist_area.height));
                        self.view.playlist_offset = super::bounded(
                            self.view.playlist_offset,
                            i64::from(delta),
                            maximum + 1,
                        );
                        return self.drag_workspace(Position::new(x, y));
                    }
                    self.view.playlist_scroll_selected = None;
                    self.focus_panel(crate::workspace::Panel::Playlists);
                    self.navigate(delta as i64);
                } else if self.view.tracks_area.contains(Position::new(x, y)) {
                    self.view.track_scroll_selected = None;
                    self.focus_panel(crate::workspace::Panel::Tracks);
                    self.navigate(delta as i64);
                }
            }
            Input::Text(text) => {
                self.finish_browser_mark_drag();
                self.view.mark_drag=None;
                if matches!(self.view.dialog, Some(Dialog::Help { .. })) {
                    self.help_search_text(&text);
                    return Ok(());
                }
                if matches!(self.view.dialog,Some(Dialog::Form(_))) { self.form_text(&text); return Ok(()); }
                if matches!(self.view.dialog, Some(Dialog::Tags(_))) {
                    self.tags_text(&text);
                    return Ok(());
                }
                if matches!(self.view.dialog, Some(Dialog::Browser(_))) {
                    return self.browser_text(&text);
                }
                if self.query_text(&text)? {
                    return self.tick_search();
                }
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    if dialog.insert_color_hex(&text) {
                        return Ok(());
                    }
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    let remaining = 512usize.saturating_sub(dialog.text.chars().count());
                    dialog
                        .text
                        .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                    dialog.sync_color_text();
                }
            }
            Input::SecondaryClick { x, y } => {
                self.finish_browser_mark_drag();
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
                        Some(Target::GroupHeader(folder) | Target::MarkGroup(folder) | Target::CollapseGroup(folder)) => {
                            self.open_group_menu(folder);
                            None
                        }
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
                self.finish_browser_mark_drag();
                self.view.mark_drag=None;
                if self.view.dialog.is_none()
                    && self.view.toolbar_selected.is_none()
                    && matches!(
                        self.view.workspace.focus,
                        crate::workspace::Panel::Playlists | crate::workspace::Panel::Tracks
                    )
                    && matches!(
                        key.key,
                        Key::Up
                            | Key::Down
                            | Key::PageUp
                            | Key::PageDown
                            | Key::Home
                            | Key::End
                            | Key::Enter
                    )
                {
                    match self.view.focus {
                        Focus::Playlists => self.view.playlist_scroll_selected = None,
                        Focus::Tracks => self.view.track_scroll_selected = None,
                    }
                }
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
        if key.key == Key::Escape && matches!(self.view.dialog, Some(Dialog::Help { .. }))
            && self.view.help_search.active()
        {
            self.clear_help_search();
            return Ok(());
        }
        if matches!(self.view.dialog, Some(Dialog::Help { .. })) && self.help_search_key(key) {
            return Ok(());
        }
        if matches!(self.view.dialog, Some(Dialog::Browser(_))) {
            return self.browser_key(key);
        }
        if self.view.dialog.is_none() && self.view.filter_editing {
            return self.filter_key(key);
        }
        if key.key == Key::Escape {
            if self.close_equalizer_presets() {
                return Ok(());
            }
            let cancelled_drag = self.cancel_workspace_drag();
            if self.close_dialog() || cancelled_drag {
                return Ok(());
            }
            if self.view.layout_editing {
                self.view.layout_editing = false;
                return Ok(());
            }
            if self.finish_playlist_editing() {
                return Ok(());
            }
            if self.view.toolbar_selected.take().is_some() {
                return Ok(());
            }
            if self.lyrics_panel_key(key)? { return Ok(()); }
            self.library.query.clear();
            self.refresh()?;
            return Ok(());
        }
        if self.lyrics_panel_key(key)? { return Ok(()); }
        let normalized = key.normalized();
        if let Some(action) = self.view.dialog.as_ref().and_then(Dialog::opening_action)
            && (self
                .settings
                .preferences
                .bindings
                .iter()
                .any(|binding| binding.key == normalized && (binding.action == action
                    || (binding.action == Action::Settings
                        && self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root))))
                || (matches!(
                    self.view.dialog,
                    Some(Dialog::SettingsHelp { .. } | Dialog::CopyableHelp { .. })
                ) && key == KeyPress::plain(Key::F(1))))
        {
            self.close_dialog();
            return Ok(());
        }
        if self.view.dialog.is_some() {
            return self.dialog_key(key);
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
            .preferences
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
        self.view.dropdown = None;
        match target {
            Target::HelpSection(index) => self.select_help_section(index),
            Target::HelpSearch => self.focus_help_search(),
            Target::HelpClearSearch => self.clear_help_search(),
            Target::HelpMatch(index) => self.open_help_match(index),
            Target::ChooseScanFolders => self.choose_scan_folders()?,
            Target::ScanFolders(action) => self.scan_folders_action(action)?,
            Target::CopyHelp(line) => {
                self.copy_help(line);
            }
            Target::Form(action) => self.form_action(action)?,
            Target::Mods(action) => {
                self.mod_action(action)?;
                if double && matches!(action,super::ModAction::Row(_)) { self.mod_action(super::ModAction::Open)?; }
            }
            Target::Services(action) => {
                self.service_action(action)?;
                if double && matches!(action,super::ServiceAction::Row(_)) { self.service_action(super::ServiceAction::Open)?; }
            }
            Target::Downloads(action) => {
                self.download_action(action)?;
                if double && matches!(action,super::DownloadAction::Row(_)) { self.download_action(super::DownloadAction::Open)?; }
            }
            Target::Lyrics(action) => {
                self.lyrics_action(action)?;
                if double && matches!(action,super::LyricsAction::Row(_)) { self.lyrics_action(super::LyricsAction::Open)?; }
            }
            Target::LyricsPanel(action) => self.lyrics_panel_action(action)?,
            Target::ToggleLyricsPanel => {
                self.show_panel(crate::workspace::Panel::Lyrics)?;
            }
            Target::Tags(action) => {
                self.tag_action(action)?;
                if double && let super::TagAction::Row(index) = action {
                    let picker = matches!(&self.view.dialog, Some(Dialog::Tags(editor)) if editor.key_picker);
                    let move_picker = matches!(&self.view.dialog, Some(Dialog::Tags(editor)) if editor.move_picker.is_some());
                    self.tag_action(if picker {
                        super::TagAction::ChooseKey(index)
                    } else if move_picker {
                        super::TagAction::ChooseDestination
                    } else {
                        super::TagAction::Edit
                    })?;
                }
            }
            Target::TogglePlayerSpecialButtons => {
                self.settings.preferences.appearance.show_player_special_buttons = !self.settings.preferences.appearance.show_player_special_buttons;
                self.cancel_workspace_drag();
                self.save_settings()?;
            }
            Target::ToggleExpandedToolbar => {
                self.action(Action::ToggleToolbar)?;
            }
            Target::CommandMenu(menu) => self.open_commands(menu),
            Target::MenuSection => {},
            Target::QuickAccess(action) => self.quick_access_action(action)?,
            Target::Visualizer(kind) => self.set_visualizer(kind)?,
            Target::StatisticsRow(row) => {
                self.statistics_target(Target::StatisticsRow(row));
                if double {
                    self.open_statistics_track()?;
                }
            }
            target @ (Target::StatisticsPage(_)
            | Target::StatisticsSort
            | Target::StatisticsRefresh) => self.statistics_target(target),
            Target::StatisticsControl(control) => self.statistics_control(control)?,
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
            Target::ListScroll(scroll) => {
                let thumb = scroll.thumb();
                let offset = if thumb.contains(self.view.pointer) {
                    scroll.offset
                } else {
                    scroll.offset_at(self.view.pointer.y, thumb.height / 2)
                };
                self.scroll_list(scroll, offset as i64 - scroll.offset as i64);
                self.view.workspace.gesture = Some(crate::workspace::Gesture::ListScroll {
                    scroll: crate::workspace::ListScroll { offset, ..scroll },
                    origin: self.view.pointer.y,
                });
            }
            Target::ResetWorkspace => self.reset_workspace()?,
            Target::Action(action) => self.action(action)?,
            Target::RevealFile(path) => self.open_file_manager(path),
            Target::Mode(mode) => self.set_mode(mode)?,
            Target::Playlist(id) => {
                self.view.playlist_scroll_selected = None;
                self.select_playlist(id);
                self.view.focus = Focus::Playlists;
                if self.playlist_movable(id) {
                    self.view.workspace.gesture =
                        Some(crate::workspace::Gesture::Playlist { id, target: None });
                }
            }
            Target::Track(id) => {
                self.view.track_scroll_selected = None;
                self.library.selected_entry = Some(id);
                self.view.focus = Focus::Tracks;
                if double {
                    self.play_selected()?;
                }
            }
            Target::Mark(id) => {
                self.view.track_scroll_selected = None;
                self.library.selected_entry = Some(id);
                self.view.focus = Focus::Tracks;
                self.start_mark_drag(id)?;
            }
            Target::MarkGroup(folder)=>{if !double{self.mark_folder_group(&folder)?;}},
            Target::GroupHeader(folder) => {
                if double { self.toggle_track_group(&folder); }
            }
            Target::GroupAction(action) => self.group_action(action)?,
            Target::CollapseGroup(folder) => {
                if !double { self.toggle_track_group(&folder); }
            }
            Target::SortColumn(sort) => {
                self.view.track_scroll_selected = None;
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
                self.view.track_scroll_selected = None;
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
            Target::PlayingTrack=>{if double{self.reveal_playing_track()?;}},
            Target::Artwork => {
                if !double {
                    self.open_artwork();
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
                self.adjust_volume(
                    percent - (self.settings.preferences.volume * 100.0).round() as i16,
                )?;
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
            Target::Submit => {
                if !double || !matches!(&self.view.dialog, Some(Dialog::Transfer(view)) if view.confirm.is_some()) { self.submit()?; }
            }
            Target::DeletePlaylistAnswer(answer) => {
                if let Some(Dialog::RemovePlaylist { confirm, .. }) = &mut self.view.dialog {
                    *confirm = answer;
                    self.submit()?;
                }
            }
            Target::ResetBindingsAnswer(answer) => {
                if let Some(Dialog::ConfirmResetBindings { confirm }) = &mut self.view.dialog {
                    *confirm = answer;
                    self.submit()?;
                }
            }
            Target::ComposeRow(index) => self.toggle_compose_row(index),
            Target::ComposeMove(direction) => self.move_composition_source(direction),
            Target::ComposeBack => self.composition_back(),
            Target::ComposeAll(all) => self.composition_all(all),
            Target::FinishEdit => {
                self.finish_playlist_editing();
            }
            Target::FolderRow(index) => {
                if let Some(Dialog::Folders { selected, .. }) = &mut self.view.dialog {
                    *selected = index;
                }
                if double && self.playlist().is_some_and(|p| p.kind == crate::model::PlaylistKind::Normal) {
                    self.edit_playlist_folder(true)?;
                }
            }
            Target::FolderAdd => self.edit_playlist_folder(false)?,
            Target::FolderEdit => self.edit_playlist_folder(true)?,
            Target::FolderRemove => self.request_folder_removal()?,
            Target::FolderScan => {
                let (id, _) = self.folder_selection()?;
                self.scan_playlist_folders(id)?;
            }
            Target::ScanSubfolders => {
                ensure!(!self.busy(), AppError::LibraryBusy);
                ensure!(
                    matches!(self.view.dialog, Some(Dialog::Folders { .. } | Dialog::ScanFolders(_)))
                        || matches!(self.view.dialog, Some(Dialog::Browser(ref browser)) if browser.folder && browser.playlist_name.is_none() && browser.palette_file.is_none() && browser.download_form.is_none()),
                    AppError::FolderNotReady
                );
                let recursive = if matches!(
                    self.view.dialog,
                    Some(Dialog::Folders { .. } | Dialog::ScanFolders(_))
                ) {
                    &mut self.settings.preferences.scan_new_subfolders
                } else {
                    &mut self.settings.preferences.scan_subfolders
                };
                *recursive = !*recursive;
                self.save_settings()?;
            }
            Target::Text(c) => {
                if let Some(Dialog::Search(search)) = &mut self.view.dialog {
                    search.focus = super::SearchFocus::Input;
                }
                if self.query_text(&c.to_string())? {
                    return Ok(());
                }
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                    if dialog.insert_color_hex(&c.to_string()) {
                        return Ok(());
                    }
                    if dialog.selected_all {
                        dialog.text.clear();
                        dialog.selected_all = false;
                    }
                    if dialog.text.chars().count() < 512 {
                        dialog.text.push(c);
                    }
                    dialog.sync_color_text();
                }
            }
            Target::Backspace => {
                if matches!(&self.view.dialog, Some(Dialog::Text(dialog)) if dialog.color_picker.is_some()) {
                    if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
                        dialog.color_picker.as_mut().unwrap().selected = None;
                    }
                    self.color_picker_key(KeyPress::plain(Key::Backspace));
                    return Ok(());
                }
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
                    dialog.sync_color_text();
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
                if !self.browser_ready() { return Ok(()); }
                let mut paint = false;
                let mut index = index;
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
                    if browser.list_area.contains(self.view.pointer) {
                        index = browser.offset + usize::from(self.view.pointer.y - browser.list_area.y);
                    }
                    browser.selected = index;
                    browser.selection_explicit = browser.entries.get(index).is_some();
                    browser.scroll_selected = None;
                    if browser.folder_playlists.is_none() {
                        if double { browser.toggle_mark(index); }
                        else { paint = true; }
                    }
                }
                if paint { self.start_browser_mark_drag(index); }
                if double {
                    self.browser_open()?;
                }
            }
            Target::BrowserMark(index) => {
                self.start_browser_mark_drag(index);
            }
            Target::BrowserSort|Target::BrowserSortDirection=>{if let Some(Dialog::Browser(browser))=&mut self.view.dialog{browser.change_sort(matches!(target,Target::BrowserSortDirection));}},
            Target::BrowserClearFilter=>{if let Some(Dialog::Browser(browser))=&mut self.view.dialog{browser.clear_filter();}},
            Target::BrowserParent => {
                self.browser_parent();
            }
            Target::BrowserHome => self.browser_location(false),
            Target::BrowserDocuments => self.browser_location(true),
            Target::BrowserMarkAll => {
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                    && browser.palette_file.is_none()
                {
                    for entry in &browser.entries {
                        if browser.can_mark(entry) {
                            browser.marked.insert(entry.path.clone());
                        }
                    }
                }
            }
            Target::BrowserClearMarks => {
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
                    browser.marked.clear();
                }
            }
            Target::BrowserOpen => self.browser_open()?,
            Target::BrowserAdd => self.browser_add()?,
            Target::BrowserCurrentFolder => self.browser_add_current_folder()?,
            Target::PaletteExportFolder => self.choose_palette_export_folder(),
            Target::PlaylistAutoName => self.choose_playlist_name_folder()?,
            Target::CreatePlaylistFolder => self.toggle_playlist_folder()?,
            Target::PlaylistFolderLocation => self.choose_playlist_folder_location()?,
            Target::CreateFolderGroups => {
                ensure!(!self.busy(), AppError::LibraryBusy);
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                    && let Some(group_subfolders) = &mut browser.folder_playlists
                {
                    *group_subfolders = !*group_subfolders;
                }
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog
                    && let super::TextPurpose::CreateFromFolder {
                        group_subfolders, ..
                    } = &mut dialog.purpose
                {
                    *group_subfolders = !*group_subfolders;
                }
            }
            Target::TransferRow(index) => {
                if let Some(Dialog::Transfer(view)) = &mut self.view.dialog
                    && view.confirm.is_none()
                {
                    view.selected = index;
                    if double { self.submit()?; }
                }
            }
            Target::TransferConfirm => {
                if !double && matches!(&self.view.dialog, Some(Dialog::Transfer(view)) if view.confirm.is_some()) {
                    self.submit_transfer()?;
                }
            }
            Target::TransferEdit => self.edit_transfer_destination()?,
            Target::TransferMove => self.open_move_files()?,
            Target::MoveFilesFolder => self.choose_move_folder(),
            Target::MoveFilesRow(index) => {
                if let Some(Dialog::MoveFiles(view)) = &mut self.view.dialog {
                    view.selected = index;
                }
                if double { self.submit_move_files()?; }
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
            Target::SettingsThemes => self.toggle_settings_themes(),
            Target::SettingsGroup(page) => self.toggle_settings_group(page),
            Target::SettingsPane => {
                self.view.settings.focus = if self.view.settings.focus == super::SettingsFocus::Parameters {
                    super::SettingsFocus::Menu
                } else {
                    super::SettingsFocus::Parameters
                };
            }
            Target::SettingsTab(page) => self.select_settings_tab(page),
            Target::SettingsGuideTopic(index) => self.select_settings_guide_topic(index),
            Target::CopyGuideCode(index) => {
                if let Some(code) = self.settings_guide_code().get(index) { self.view.clipboard_text = Some(code.clone()); }
            }
            Target::SettingsVolume(index, area) => {
                if area.width > 0 {
                    if self.view.workspace.gesture.is_none() {
                        self.view.workspace.gesture =
                            Some(crate::workspace::Gesture::Volume(index, area));
                    }
                    self.select_setting(index);
                    let point = self.last_pointer_x(area);
                    self.set_volume(
                        f32::from(point) / f32::from(area.width.saturating_sub(1).max(1)),
                    )?;
                }
            }
            Target::GifOpacity(index, area) => {
                let point = self.last_pointer_x(area);
                self.drag_gif_opacity(index, area, f64::from(point) / f64::from(area.width.saturating_sub(1).max(1)), false);
            }
            Target::SettingHelp(index) => self.settings_help(index),
            Target::SettingsTimeline(index, choice) => {
                self.select_setting(index);
                self.set_playback_timeline(choice)?;
            }
            Target::BindingModifier(index) => {
                if matches!(self.view.dialog, Some(Dialog::CaptureBinding { .. }))
                    && let Some(value) = self.view.settings.binding_modifiers.get_mut(index)
                {
                    *value = !*value;
                    self.view.settings.binding_global_focus = None;
                }
            }
            Target::BindingGlobal(index) => {
                self.view.settings.binding_global_focus = Some(0);
                self.toggle_global_shortcut(index)?;
            }
            Target::BindingGlobalEdit(index) => self.edit_global_shortcut(index),
            Target::BindingKey(key) => {
                if matches!(self.view.dialog, Some(Dialog::CaptureBinding { .. })) {
                    self.view.settings.binding_global_focus = None;
                    let [ctrl, alt, shift] = self.view.settings.binding_modifiers;
                    self.key(KeyPress {
                        key,
                        ctrl,
                        alt,
                        shift,
                    })?;
                }
            }
            target @ (Target::EqualizerToggle
            | Target::EqualizerPresets
            | Target::EqualizerPreset(_)
            | Target::EqualizerSelect(_)
            | Target::EqualizerGain { .. }
            | Target::EqualizerReset) => self.equalizer_target(target)?,
            target @ (Target::TunerSelect(_)
            | Target::TunerSlider(_, _)
            | Target::TunerReverse
            | Target::TunerReset
            | Target::TunerPin) => self.tuner_target(target)?,
            Target::ColorHex(area) => self.click_color_hex(area, double),
            target @ (Target::ColorWheel(_)
            | Target::ColorSlider(_, _)
            | Target::ColorSelect(_)
            | Target::ColorReset) => self.color_target(target),
        }
        Ok(())
    }

    fn reverse_target(&mut self, target: Target) -> Result<()> {
        match target {
            Target::Setting(index)
            | Target::SettingSelect(index)
            | Target::SettingsVolume(index, _)
            | Target::GifOpacity(index, _) => self.adjust_setting(index, -1)?,
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
            Target::SettingsThemes if self.view.settings.themes_expanded => {
                self.toggle_settings_themes();
            }
            Target::SettingsGroup(page) if self.settings_expanded(page) => {
                self.toggle_settings_group(page);
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

    fn wheel_step(&self, position: Position, ctrl: bool, shift: bool, normal: i16) -> i16 {
        if ctrl && shift {
            self.wheel_page_size(position).clamp(1, i16::MAX as usize) as i16
        } else if ctrl {
            normal * 5
        } else {
            normal
        }
    }

    fn shift_dialog_page(&mut self, delta: i16) {
        if self.settings_guide().is_some() && matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
            return;
        }
        if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root)
            && self.view.settings.focus != super::SettingsFocus::Parameters
        { return; }
        let offset = match &mut self.view.dialog {
            Some(Dialog::Browser(view)) => Some(&mut view.offset),
            Some(Dialog::Search(view)) => Some(if view.focus == super::SearchFocus::Playlists {
                &mut view.playlist_offset
            } else {
                &mut view.offset
            }),
            Some(Dialog::Settings { .. })
                if self.view.settings.focus == super::SettingsFocus::Parameters => {
                Some(&mut self.view.settings.offset)
            }
            Some(Dialog::Tags(view)) => Some(&mut view.offset),
            Some(Dialog::Downloads(view)) => Some(&mut view.offset),
            Some(Dialog::Services(view)) => Some(&mut view.offset),
            Some(Dialog::Lyrics(view)) if !view.content => Some(&mut view.offset),
            Some(Dialog::Mods(view)) if !view.opened && view.install.is_none() => {
                Some(&mut view.offset)
            }
            _ => None,
        };
        if let Some(offset) = offset {
            *offset = offset.saturating_add_signed(isize::from(delta));
        }
    }

    fn wheel_page_size(&self, position: Position) -> usize {
        let count = |predicate: fn(&Target) -> bool| {
            self.view.hits.iter()
                .filter(|hit| predicate(&hit.target))
                .count()
        };
        if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root)
            && self.view.settings.menu_area.contains(position)
        {
            return count(|target| matches!(target, Target::SettingsPage(_) | Target::SettingsTab(_) | Target::SettingsThemes | Target::SettingsGroup(_)));
        }
        if matches!(self.view.dialog, Some(Dialog::Settings { .. })) && self.settings_guide().is_some() {
            return self.view.dialog_page_size.max(1);
        }
        match &self.view.dialog {
            None => {
                if let Some(Target::ListScroll(scroll)) = self.target_at(position) {
                    scroll.visible
                } else if self.view.playlist_area.contains(position) {
                    usize::from(self.view.playlist_area.height)
                } else if self.view.tracks_area.contains(position) {
                    count(|target| matches!(target, Target::Track(_) | Target::GroupHeader(_)))
                } else {
                    1
                }
            }
            Some(Dialog::Statistics) => self.statistics.view.capacity,
            Some(Dialog::Search(search)) => {
                if search.playlists_area.contains(position) {
                    count(|target| matches!(target, Target::SearchPlaylist(_)))
                } else {
                    count(|target| matches!(target, Target::SearchResult(_)))
                }
            }
            Some(Dialog::Settings { .. }) => {
                if self.view.settings.menu_area.contains(position) {
                    count(|target| matches!(
                        target,
                        Target::SettingsPage(_) | Target::SettingsTab(_) | Target::SettingsThemes
                    ))
                } else {
                    self.view.hits.iter()
                        .filter_map(|hit| match hit.target {
                            Target::Setting(index) | Target::SettingSelect(index) => Some(index),
                            _ => None,
                        })
                        .collect::<std::collections::HashSet<_>>()
                        .len()
                }
            }
            Some(Dialog::CaptureBinding { .. }) => {
                count(|target| matches!(target, Target::BindingKey(_)))
            }
            Some(_) => self.view.dialog_page_size.max(count(|target| matches!(
                target,
                Target::BrowserRow(_)
                    | Target::FolderRow(_)
                    | Target::TransferRow(_)
                    | Target::MoveFilesRow(_)
                    | Target::ComposeRow(_)
                    | Target::PanelRow(_)
                    | Target::CommandRow(_)
                    | Target::EqualizerPreset(_)
                    | Target::Tags(super::TagAction::Row(_))
                    | Target::Lyrics(super::LyricsAction::Row(_))
                    | Target::Downloads(super::DownloadAction::Row(_))
                    | Target::Services(super::ServiceAction::Row(_))
                    | Target::Mods(super::ModAction::Row(_))
                    | Target::ScanFolders(super::ScanFolderAction::Row(_))
            ))),
        }
    }
}
