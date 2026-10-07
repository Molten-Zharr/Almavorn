use super::{App, Browser, SettingsCatalog, SettingsEdit, bounded, database::DatabaseOutcome};
use crate::errors::AppError;
use crate::model::{Language, Playlist, Track};
use anyhow::{Context, Result};

#[derive(Clone)]
pub enum TextPurpose {
    Create,
    CreateFromFolder {
        folder: std::path::PathBuf,
        group_subfolders: bool,
    },
    ComposePlaylist(Vec<i64>),
    RenamePlaylist(i64),
    PlaylistFolder(i64, Option<String>),
    RemovePlaylistFolder(i64, String),
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
    Commands {
        menu: super::CommandMenu,
        selected: usize,
        anchor: ratatui::layout::Position,
    },
    Panels {
        selected: usize,
        expanded: Vec<crate::workspace::Panel>,
    },
    Text(TextDialog),
    Search(super::search::SearchDialog),
    Browser(Browser),
    Folders {
        playlist: i64,
        selected: usize,
    },
    ComposePlaylists {
        selected: usize,
        ids: Vec<i64>,
    },
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
    pub(super) fn opening_action(&self) -> Option<crate::input::Action> {
        use crate::input::Action;
        match self {
            Self::Panels { .. } => Some(Action::Panels),
            Self::Help { .. } | Self::SettingsHelp { .. } => Some(Action::Help),
            Self::Settings { .. } => Some(Action::Settings),
            Self::Browser(browser) if browser.playlist_name.is_some() => None,
            Self::Browser(browser) => Some(if browser.folder {
                Action::AddFolder
            } else {
                Action::AddFiles
            }),
            Self::Metadata { .. } => Some(Action::Metadata),
            Self::Folders { .. } => Some(Action::PlaylistFolders),
            Self::ComposePlaylists { .. } => Some(Action::ComposePlaylists),
            Self::Transfer { .. } => Some(Action::Transfer),
            Self::RemoveEntry { .. } | Self::ConfirmSettings { .. } => Some(Action::Delete),
            Self::Text(_) | Self::CaptureBinding { .. } | Self::Commands { .. } => None,
            Self::Search(_) => None,
        }
    }

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
        self.view.dialog = Some(Dialog::Text(TextDialog {
            purpose,
            text,
            keyboard: self.settings.language,
            upper: false,
            selected_all,
        }));
    }

    pub(super) fn scroll_dialog(&mut self, direction: i16) {
        if matches!(self.view.dialog, Some(Dialog::Search(_))) {
            self.search_navigate(i64::from(direction));
            return;
        }
        if let Some(Dialog::Commands { menu, selected, .. }) = &self.view.dialog {
            let next = Self::command_selection(
                &self.command_items(*menu),
                *selected,
                i64::from(direction),
            );
            if let Some(Dialog::Commands { selected, .. }) = &mut self.view.dialog {
                *selected = next;
            }
            return;
        }
        let transfer_length = self.transfer_destinations().len();
        let compose_length = self.compose_choices().len();
        let folder_length = if let Some(Dialog::Folders { playlist, .. }) = &self.view.dialog {
            self.library
                .playlists
                .iter()
                .find(|item| item.id == *playlist)
                .map_or(0, |item| item.folders.len())
        } else {
            0
        };
        if matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
            self.settings_scroll(i64::from(direction));
            return;
        }
        match &mut self.view.dialog {
            Some(Dialog::ComposePlaylists { selected, .. }) => {
                *selected = bounded(*selected, i64::from(direction), compose_length)
            }
            Some(Dialog::Folders { selected, .. }) => {
                *selected = bounded(*selected, i64::from(direction), folder_length)
            }
            Some(Dialog::Panels { selected, expanded }) => {
                *selected = bounded(
                    *selected,
                    i64::from(direction),
                    crate::workspace::panel_rows(expanded).len(),
                )
            }
            Some(Dialog::Browser(browser)) => {
                browser.selected =
                    bounded(browser.selected, direction as i64, browser.entries.len())
            }
            Some(Dialog::CaptureBinding { .. }) => {
                self.view.settings.binding_offset = bounded(
                    self.view.settings.binding_offset,
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
                *offset = bounded(
                    *offset,
                    direction as i64,
                    self.view.dialog_scroll_max.saturating_add(1),
                )
            }
            _ => {}
        }
    }

    pub fn transfer_destinations(&self) -> Vec<&Playlist> {
        self.library
            .playlists
            .iter()
            .filter(|playlist| {
                Some(playlist.id) != self.library.selected_playlist
                    && playlist.can_edit(self.view.editing)
            })
            .collect()
    }

    pub(super) fn submit(&mut self) -> Result<()> {
        if let Some(Dialog::Commands { selected, .. }) = &self.view.dialog {
            return self.activate_command(*selected);
        }
        if matches!(self.view.dialog, Some(Dialog::Search(_))) {
            return self.play_search_result();
        }
        match self.view.dialog.take() {
            Some(Dialog::ComposePlaylists { selected, ids }) => {
                if ids.len() < 2 {
                    self.view.dialog = Some(Dialog::ComposePlaylists { selected, ids });
                    anyhow::bail!(
                        "{}",
                        self.text(
                            "Select at least two playlists",
                            "Отметьте минимум два плейлиста."
                        )
                    );
                }
                let name = self.fresh_playlist_name(self.text("Compilation", "Сборка"));
                self.text_dialog(TextPurpose::ComposePlaylist(ids), name);
            }
            Some(Dialog::Panels { selected, expanded }) => {
                self.view.dialog = Some(Dialog::Panels { selected, expanded });
                self.activate_panel_row(selected, false)?;
            }
            Some(Dialog::Text(dialog)) => {
                let recovery = Some(Dialog::Text(dialog.clone()));
                let text = dialog.text.clone();
                let mode = self.settings.mode;
                let unlocked = self.view.editing;
                let result = match &dialog.purpose {
                    TextPurpose::Create => self.start_database(recovery, move |store| {
                        Ok(DatabaseOutcome::Created(
                            store.create_playlist(&text, mode)?,
                        ))
                    }),
                    TextPurpose::CreateFromFolder {
                        folder,
                        group_subfolders,
                    } => self.start_folder_playlist(
                        text,
                        folder.clone(),
                        *group_subfolders,
                        recovery,
                    ),
                    TextPurpose::ComposePlaylist(ids) => {
                        let ids = ids.clone();
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Created(
                                store.compose_playlists(&ids, &text, mode, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::RenamePlaylist(id) => {
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.rename_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::PlaylistFolder(id, previous) => {
                        let id = *id;
                        let previous = previous.clone();
                        let folder = std::path::PathBuf::from(text.trim());
                        let result = self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(store.set_playlist_folder(
                                id,
                                previous.as_deref(),
                                &folder,
                                unlocked,
                            )?))
                        });
                        if result.is_ok() {
                            self.view.dialog = Some(Dialog::Folders {
                                playlist: id,
                                selected: 0,
                            });
                        }
                        result
                    }
                    TextPurpose::RemovePlaylistFolder(id, folder) => {
                        let id = *id;
                        let folder = folder.clone();
                        let result = self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.remove_playlist_folder(id, &folder, &text, unlocked)?,
                            ))
                        });
                        if result.is_ok() {
                            self.view.dialog = Some(Dialog::Folders {
                                playlist: id,
                                selected: 0,
                            });
                        }
                        result
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
                            self.view.dialog = Some(Dialog::Text(dialog));
                            anyhow::bail!(AppError::PlaylistConfirmationRequired);
                        }
                        let id = *id;
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.delete_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::Search => {
                        self.library.query = dialog.text.clone();
                        self.view.track_offset = 0;
                        self.refresh()
                    }
                    TextPurpose::Accent => {
                        let value = dialog.text.trim().trim_start_matches('#');
                        let color = u32::from_str_radix(value, 16)
                            .ok()
                            .filter(|_| value.len() == 6)
                            .context(AppError::InvalidLegacyColor);
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
                    self.view.dialog = Some(Dialog::Text(dialog));
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
                let unlocked = self.view.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.remove_entry(playlist, entry, unlocked)?,
                    ))
                }) {
                    self.view.dialog = Some(recovery);
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
                let unlocked = self.view.editing;
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(store.copy_tracks(
                        destination,
                        &ids,
                        unlocked,
                    )?))
                }) {
                    self.view.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::Browser(browser)) => {
                self.view.dialog = Some(Dialog::Browser(browser));
                self.browser_open()?;
            }
            Some(Dialog::Settings { selected }) => self.setting(selected)?,
            Some(Dialog::ConfirmSettings { catalog, id, name }) => {
                if let Err(error) = self.delete_settings_item(catalog, &id) {
                    self.view.dialog = Some(Dialog::ConfirmSettings { catalog, id, name });
                    return Err(error);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
