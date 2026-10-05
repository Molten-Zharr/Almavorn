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

    pub fn handle(&mut self, input: Input) {
        if let Err(error) = self.handle_inner(input) {
            self.failure(error);
        }
    }

    fn handle_inner(&mut self, input: Input) -> Result<()> {
        match input {
            Input::Move { x, y } => self.pointer = Position::new(x, y),
            Input::Click { x, y, double } => {
                self.pointer = Position::new(x, y);
                let hit = self
                    .hits
                    .iter()
                    .rev()
                    .find(|hit| hit.enabled && hit.area.contains(Position::new(x, y)))
                    .map(|hit| hit.target.clone());
                if let Some(target) = hit {
                    self.target(target, double)?;
                }
            }
            Input::Scroll { x, y, delta } => {
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
            Input::Key(key) => self.key(key)?,
        }
        Ok(())
    }

    fn key(&mut self, key: KeyPress) -> Result<()> {
        if key.key == Key::Escape {
            if self.dialog.take().is_none() {
                self.query.clear();
                self.refresh()?;
            }
            return Ok(());
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
            self.dialog = Some(Dialog::Settings {
                selected: 6 + index,
            });
            return Ok(());
        }
        if self.dialog.is_some() {
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
        let normalized = key.normalized();
        if let Some(action) = self
            .settings
            .bindings
            .iter()
            .find(|binding| binding.key == normalized)
            .map(|binding| binding.action)
        {
            self.action(action)?;
            return Ok(());
        }
        match key.key {
            Key::Tab => {
                self.focus = if self.focus == Focus::Tracks {
                    Focus::Playlists
                } else {
                    Focus::Tracks
                }
            }
            Key::Up => self.navigate(-1),
            Key::Down => self.navigate(1),
            Key::PageUp => self.navigate(-10),
            Key::PageDown => self.navigate(10),
            Key::Home => self.navigate(i64::MIN),
            Key::End => self.navigate(i64::MAX),
            Key::Enter => {
                if self.focus == Focus::Playlists {
                    self.focus = Focus::Tracks;
                } else {
                    self.play_selected()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn target(&mut self, target: Target, double: bool) -> Result<()> {
        match target {
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
            Target::CloseDialog => self.dialog = None,
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
                if let Some(Dialog::Browser(browser)) = &mut self.dialog
                    && let Some(parent) = browser.directory.parent()
                {
                    browser.directory = parent.to_owned();
                    browser.refresh()?;
                }
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
        }
        Ok(())
    }

    fn last_pointer_x(&self, area: Rect) -> u16 {
        self.pointer.x.saturating_sub(area.x).min(area.width)
    }
}
