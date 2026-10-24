use super::super::{App, Dialog, SettingsEdit, Target, TextPurpose};
use crate::input::{Key, KeyPress};
use anyhow::Result;

impl App {
    pub(super) fn text_key(&mut self, key: KeyPress) -> Result<()> {
        if self.color_picker_key(key) {
            return Ok(());
        }
        if key.ctrl
            && !key.alt
            && let Some(Dialog::Text(dialog)) = &self.view.dialog
        {
            match (key.key, &dialog.purpose) {
                (
                    Key::Char('d'),
                    TextPurpose::Create
                    | TextPurpose::CreateWithFolder { .. }
                    | TextPurpose::CreateFromFolder { .. },
                ) => {
                    return self.target(Target::CreatePlaylistFolder, false);
                }
                (Key::Char('f'), TextPurpose::CreateWithFolder { .. }) => {
                    return self.choose_playlist_folder_location();
                }
                (Key::Char('g'), TextPurpose::CreateFromFolder { .. }) => {
                    return self.target(Target::CreateFolderGroups, false);
                }
                (Key::Char('f'), TextPurpose::Create | TextPurpose::CreateFromFolder { .. }) => {
                    return self.choose_playlist_name_folder();
                }
                (Key::Char('f'), TextPurpose::Settings(SettingsEdit::ExportPalette { .. })) => {
                    self.choose_palette_export_folder();
                    return Ok(());
                }
                _ => {}
            }
        }
        if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
            if key.ctrl && key.key == Key::Char('a') {
                dialog.selected_all = true;
                return Ok(());
            }
            if key.key == Key::Backspace {
                if dialog.selected_all {
                    dialog.text.clear();
                    dialog.selected_all = false;
                } else {
                    dialog.text.pop();
                }
                dialog.sync_color_text();
                return Ok(());
            }
        }
        self.dialog_navigation_key(key)
    }
}
