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
        match self.view.dialog.as_ref() {
            Some(Dialog::Settings { .. }) => self.settings_key(key),
            Some(Dialog::Equalizer { .. }) => self.equalizer_key(key.normalized()),
            Some(Dialog::PlaybackTuner { .. }) => self.tuner_key(key.normalized()),
            Some(Dialog::Statistics) => self.statistics_key(key.normalized()),
            Some(Dialog::CaptureBinding { index }) => self.capture_binding_key(*index, key),
            Some(Dialog::Text(_)) => self.text_key(key),
            Some(Dialog::RemovePlaylist { .. }) => self.remove_playlist_key(key),
            Some(Dialog::ComposePlaylists { .. }) => self.composition_key(key),
            Some(Dialog::Folders { .. }) => self.folders_key(key),
            Some(Dialog::Search(_)) => self.search_key(key),
            Some(Dialog::Commands { .. }) => self.commands_key(key),
            Some(Dialog::Panels { .. }) => self.panels_key(key),
            Some(Dialog::Browser(_)) => self.browser_key(key),
            Some(
                Dialog::RemoveEntry { .. }
                | Dialog::Transfer { .. }
                | Dialog::Help { .. }
                | Dialog::ConfirmSettings { .. }
                | Dialog::SettingsHelp { .. }
                | Dialog::Metadata { .. },
            ) => self.dialog_navigation_key(key),
            None => Ok(()),
        }
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
