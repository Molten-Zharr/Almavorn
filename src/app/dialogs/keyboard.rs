use super::{App, Dialog};
use crate::{
    app::Target,
    input::{Key, KeyPress},
};
use anyhow::Result;

impl App {
    /// Dispatch after global cancellation and the dialog's opening shortcut.
    /// Every dialog owns its remaining keys, including keys it ignores.
    pub(in crate::app) fn dialog_key(&mut self, key: KeyPress) -> Result<()> {
        if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root) {
            return self.settings_key(key);
        }
        match self.view.dialog.as_ref() {
            Some(Dialog::Help { .. }) => self.help_key(key),
            Some(Dialog::ScanFolders(_)) => self.scan_folders_key(key),
            Some(Dialog::Form(_)) => self.form_key(key),
            Some(Dialog::Lyrics(_)) => self.lyrics_key(key),
            Some(Dialog::Downloads(_)) => self.downloads_key(key),
            Some(Dialog::Services(_)) => self.services_key(key),
            Some(Dialog::Mods(_)) => self.mods_key(key),
            Some(Dialog::CopyableHelp { .. } | Dialog::FileMoveReport { .. })
                if key.ctrl && key.key == Key::Char('c') || key.key == Key::Enter =>
            {
                self.copy_help(None);
                Ok(())
            }
            Some(Dialog::Tags(_)) => self.tags_key(key),
            Some(Dialog::Settings { .. }) => self.settings_key(key),
            Some(Dialog::Equalizer { .. }) => self.equalizer_key(key.normalized()),
            Some(Dialog::PlaybackTuner { .. }) => self.tuner_key(key.normalized()),
            Some(Dialog::Statistics) => self.statistics_key(key.normalized()),
            Some(Dialog::CaptureBinding { index }) => self.capture_binding_key(*index, key),
            Some(Dialog::Text(_)) => self.text_key(key),
            Some(Dialog::RemovePlaylist { .. }) => self.remove_playlist_key(key),
            Some(Dialog::ConfirmResetBindings { confirm }) => {
                let confirm = *confirm;
                if !key.ctrl && !key.alt {
                    match key.key {
                        Key::Left | Key::Up | Key::Right | Key::Down | Key::Tab => {
                            let confirm = match key.key {
                                Key::Left | Key::Up => true,
                                Key::Right | Key::Down => false,
                                _ => !confirm,
                            };
                            self.view.dialog = Some(Dialog::ConfirmResetBindings { confirm });
                        }
                        Key::Enter | Key::Char(' ') => self.submit()?,
                        _ => {}
                    }
                }
                Ok(())
            }
            Some(Dialog::ComposePlaylists { .. } | Dialog::ComposeTracks { .. }) => {
                self.composition_key(key)
            }
            Some(Dialog::Folders { .. }) => self.folders_key(key),
            Some(Dialog::Search(_)) => self.search_key(key),
            Some(Dialog::Commands { .. }) => self.commands_key(key),
            Some(Dialog::Panels { .. }) => self.panels_key(key),
            Some(Dialog::QuickAccess { .. }) => self.quick_access_key(key),
            Some(Dialog::Browser(_)) => self.browser_key(key),
            Some(Dialog::Transfer(_) | Dialog::MoveFiles(_)) => self.transfer_key(key),
            Some(
                Dialog::RemoveEntry { .. }
                | Dialog::RemoveGroup { .. }
                | Dialog::ConfirmSettings { .. }
                | Dialog::SettingsHelp { .. }
                | Dialog::CopyableHelp { .. }
                | Dialog::FileMoveReport { .. }
                | Dialog::Metadata { .. },
            ) => self.dialog_navigation_key(key),
            None => Ok(()),
        }
    }

    pub(in crate::app) fn select_help_section(&mut self, index: usize) {
        if self.view.help_search.active() { self.clear_help_search(); }
        if let Some(Dialog::Help { offset }) = &mut self.view.dialog
            && let Some(start) = self.view.help_sections.get(index)
        {
            *offset = (*start).min(self.view.dialog_scroll_max);
        }
    }

    pub(in crate::app) fn move_help_section(&mut self, direction: i64) {
        let Some(Dialog::Help { offset }) = &self.view.dialog else { return; };
        let count = self.view.help_sections.len();
        if count == 0 { return; }
        let current = if *offset == self.view.dialog_scroll_max && *offset > 0 { count - 1 }
            else { self.view.help_sections.partition_point(|start| *start <= *offset).saturating_sub(1) };
        self.select_help_section(super::super::bounded(current, direction, count));
    }

    fn help_key(&mut self, key: KeyPress) -> Result<()> {
        match key.key {
            Key::Left => self.move_help_section(-1),
            Key::Right => self.move_help_section(1),
            Key::Tab => self.move_help_section(if key.shift { -1 } else { 1 }),
            Key::Up => self.scroll_dialog(-1),
            Key::Down => self.scroll_dialog(1),
            Key::PageUp | Key::PageDown => {
                let page = self.view.dialog_page_size.min(i16::MAX as usize).max(1) as i16;
                self.scroll_dialog(if key.key == Key::PageUp { -page } else { page });
            }
            Key::Home | Key::End => {
                if let Some(Dialog::Help { offset }) = &mut self.view.dialog {
                    *offset = if key.key == Key::Home { 0 } else { self.view.dialog_scroll_max };
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn dialog_navigation_key(&mut self, key: KeyPress) -> Result<()> {
        match key.key {
            Key::Enter => self.submit()?,
            Key::Up => self.scroll_dialog(-1),
            Key::Down => self.scroll_dialog(1),
            Key::PageUp => self.scroll_dialog(-8),
            Key::PageDown => self.scroll_dialog(8),
            _ => {}
        }
        Ok(())
    }

    fn remove_playlist_key(&mut self, key: KeyPress) -> Result<()> {
        if let Some(Dialog::RemovePlaylist { confirm, .. }) = &mut self.view.dialog
            && !key.ctrl
            && !key.alt
        {
            match key.key {
                Key::Left | Key::Up => *confirm = true,
                Key::Right | Key::Down => *confirm = false,
                Key::Tab => *confirm = !*confirm,
                Key::Char('y' | 'Y' | 'д' | 'Д') => {
                    return self.target(Target::DeletePlaylistAnswer(true), false);
                }
                Key::Char('n' | 'N' | 'н' | 'Н') => {
                    return self.target(Target::DeletePlaylistAnswer(false), false);
                }
                Key::Enter => self.submit()?,
                _ => {}
            }
            return Ok(());
        }
        self.dialog_navigation_key(key)
    }
}
