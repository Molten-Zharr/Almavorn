use super::{App, Browser, SettingsCatalog, SettingsEdit, bounded, database::DatabaseOutcome};
use crate::model::{Language, Playlist, Track};
use anyhow::{Context, Result};

#[derive(Clone)]
pub enum TextPurpose {
    Create,
    RenamePlaylist(i64),
    RenameTrack(i64, i64),
    Search,
    DeletePlaylist(i64, String),
    Accent,
    Settings(SettingsEdit),
}

#[derive(Clone)]
pub struct TextDialog {
    pub purpose: TextPurpose,
    pub text: String,
    pub keyboard: Language,
    pub upper: bool,
    pub selected_all: bool,
}

#[derive(Clone)]
pub enum Dialog {
    Text(TextDialog),
    Browser(Browser),
    RemoveEntry {
        playlist: i64,
        entry: i64,
        title: String,
    },
    Transfer {
        ids: Vec<i64>,
        selected: usize,
    },
    Help {
        offset: usize,
    },
    Settings {
        selected: usize,
    },
    ConfirmSettings {
        catalog: SettingsCatalog,
        id: String,
        name: String,
    },
    SettingsHelp {
        title: String,
        description: String,
        offset: usize,
    },
    Metadata {
        track: Track,
        offset: usize,
    },
    CaptureBinding {
        index: usize,
    },
}

impl Dialog {
    pub(crate) fn is_settings(&self) -> bool {
        matches!(
            self,
            Self::Settings { .. }
                | Self::Text(TextDialog {
                    purpose: TextPurpose::Settings(_),
                    ..
                })
                | Self::ConfirmSettings { .. }
                | Self::SettingsHelp { .. }
                | Self::CaptureBinding { .. }
        )
    }
}

impl App {
    pub(super) fn text_dialog(&mut self, purpose: TextPurpose, text: String) {
        let selected_all = !text.is_empty();
        self.dialog = Some(Dialog::Text(TextDialog {
            purpose,
            text,
            keyboard: self.settings.language,
            upper: false,
            selected_all,
        }));
    }

    pub(super) fn scroll_dialog(&mut self, direction: i16) {
        let transfer_length = self.transfer_destinations().len();
        if matches!(self.dialog, Some(Dialog::Settings { .. })) {
            self.settings_scroll(i64::from(direction));
            return;
        }
        match &mut self.dialog {
            Some(Dialog::Browser(browser)) => {
                browser.selected =
                    bounded(browser.selected, direction as i64, browser.entries.len())
            }
            Some(Dialog::CaptureBinding { .. }) => {
                self.settings_view.binding_offset = bounded(
                    self.settings_view.binding_offset,
                    i64::from(direction) * 8,
                    crate::input::Key::shortcut_choices().count(),
                );
            }
            Some(Dialog::Transfer { selected, .. }) => {
                *selected = bounded(*selected, direction as i64, transfer_length)
            }
            Some(Dialog::Help { offset })
            | Some(Dialog::Metadata { offset, .. })
            | Some(Dialog::SettingsHelp { offset, .. }) => {
                *offset = bounded(*offset, direction as i64, 1000)
            }
            _ => {}
        }
    }

    pub fn transfer_destinations(&self) -> Vec<&Playlist> {
        self.playlists
            .iter()
            .filter(|playlist| {
                Some(playlist.id) != self.selected_playlist && playlist.can_edit(self.editing)
            })
            .collect()
    }

    pub(super) fn submit(&mut self) -> Result<()> {
        match self.dialog.take() {
            Some(Dialog::Text(dialog)) => {
                let recovery = Some(Dialog::Text(dialog.clone()));
                let text = dialog.text.clone();
                let mode = self.settings.mode;
                let unlocked = self.editing;
                let result = match &dialog.purpose {
                    TextPurpose::Create => self.start_database(recovery, move |store| {
                        Ok(DatabaseOutcome::Created(
                            store.create_playlist(&text, mode)?,
                        ))
                    }),
                    TextPurpose::RenamePlaylist(id) => {
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.rename_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::RenameTrack(track, playlist) => {
                        let (track, playlist) = (*track, *playlist);
                        self.start_database(recovery, move |store| {
                            store.rename_track(track, &text, playlist, unlocked)?;
                            Ok(DatabaseOutcome::Renamed)
                        })
                    }
                    TextPurpose::DeletePlaylist(id, expected) => {
                        if &dialog.text != expected {
                            self.dialog = Some(Dialog::Text(dialog));
                            anyhow::bail!("Enter the exact playlist name");
                        }
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.delete_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::Search => {
                        self.query = dialog.text.clone();
                        self.track_offset = 0;
                        self.refresh()
                    }
                    TextPurpose::Accent => {
                        let value = dialog.text.trim().trim_start_matches('#');
                        let color = u32::from_str_radix(value, 16)
                            .ok()
                            .filter(|_| value.len() == 6)
                            .context("Use a six-digit color, for example E89E4A");
                        color.and_then(|color| {
                            self.settings.themes[self.settings.mode.index()].accent =
                                [(color >> 16) as u8, (color >> 8) as u8, color as u8];
                            self.save_settings()
                        })
                    }
                    TextPurpose::Settings(edit) => {
                        self.finish_settings_edit(edit.clone(), &dialog.text)
                    }
                };
                if let Err(error) = result {
                    self.dialog = Some(Dialog::Text(dialog));
                    return Err(error);
                }
            }
            Some(Dialog::RemoveEntry {
                playlist,
                entry,
                title,
            }) => {
                let recovery = Dialog::RemoveEntry {
                    playlist,
                    entry,
                    title,
                };
                let unlocked = self.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.remove_entry(playlist, entry, unlocked)?,
                    ))
                }) {
                    self.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::Transfer { ids, selected }) => {
                let destination = self
                    .transfer_destinations()
                    .get(selected)
                    .map(|playlist| playlist.id)
                    .context("No editable destination playlist. Enable Order editing if needed")?;
                let recovery = Dialog::Transfer {
                    ids: ids.clone(),
                    selected,
                };
                let unlocked = self.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(store.copy_tracks(
                        destination,
                        &ids,
                        unlocked,
                    )?))
                }) {
                    self.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::Browser(browser)) => {
                self.dialog = Some(Dialog::Browser(browser));
                self.browser_open()?;
            }
            Some(Dialog::Settings { selected }) => self.setting(selected)?,
            Some(Dialog::ConfirmSettings { catalog, id, name }) => {
                if let Err(error) = self.delete_settings_item(catalog, &id) {
                    self.dialog = Some(Dialog::ConfirmSettings { catalog, id, name });
                    return Err(error);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
