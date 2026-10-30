mod keyboard;
mod text;

use super::{App, Browser, SettingsCatalog, SettingsEdit, bounded, database::DatabaseOutcome};
use crate::errors::AppError;
use crate::model::{Language, Playlist, Track};
use anyhow::{Context, Result};

#[derive(Clone)]
pub enum TextPurpose {
    Create,
    CreateWithFolder {
        parent: std::path::PathBuf,
    },
    CreateFromFolder {
        folder: std::path::PathBuf,
        group_subfolders: bool,
    },
    ComposePlaylist(Vec<i64>),
    ComposeSelection {
        sources: Vec<i64>,
        entries: Vec<i64>,
        available: Vec<i64>,
    },
    RenamePlaylist(i64),
    PlaylistFolder(i64, Option<String>),
    RemovePlaylistFolder(i64, String),
    RenameTrack(i64, i64),
    Search,
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
    pub color_picker: Option<crate::color_picker::ColorPicker>,
}

impl TextDialog {
    pub(super) fn sync_color_text(&mut self) {
        if let Some(color) = &mut self.color_picker {
            color.selected = None;
            let length = self.text.chars().count();
            color.hex_cursor = color.hex_cursor.min(length);
            color.hex_anchor = color.hex_anchor.min(length);
            if let Ok(rgb) = crate::preferences::parse_color(&self.text) {
                color.set_rgb(rgb);
            }
        }
    }
}

#[derive(Clone)]
pub enum Dialog {
    ArtworkSearch,
    Form(Box<super::Form>),
    Lyrics(super::LyricsView),
    Downloads(super::DownloadsView),
    Services(super::ServicesView),
    Mods(super::ModsView),
    Tags(Box<super::TagEditor>),
    Statistics,
    PlaybackTuner {
        selected: usize,
    },
    Equalizer {
        selected: usize,
        presets: bool,
        preset_selected: usize,
    },
    Commands {
        menu: super::CommandMenu,
        selected: usize,
        anchor: ratatui::layout::Position,
    },
    Panels {
        selected: usize,
        expanded: Vec<crate::workspace::Panel>,
    },
    QuickAccess { selected: usize },
    Text(TextDialog),
    Search(super::search::SearchDialog),
    Browser(Browser),
    ScanFolders(super::ScanFolders),
    Folders {
        playlist: i64,
        selected: usize,
    },
    ComposePlaylists {
        selected: usize,
        ids: Vec<i64>,
    },
    ComposeTracks {
        sources: Vec<i64>,
        selected: usize,
        entries: Vec<i64>,
        included: std::collections::HashSet<i64>,
    },
    RemoveEntry {
        playlist: i64,
        entry: i64,
        title: String,
    },
    RemoveGroup {
        playlist: i64,
        folder: std::path::PathBuf,
        name: String,
        entries: Vec<i64>,
    },
    RemovePlaylist {
        playlist: i64,
        name: String,
        confirm: bool,
    },
    Transfer(super::TransferView),
    MoveFiles(super::MoveFiles),
    FileMoveReport {
        title: String,
        description: String,
        offset: usize,
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
    ConfirmResetBindings {
        confirm: bool,
    },
    SettingsHelp {
        title: String,
        description: String,
        offset: usize,
    },
    CopyableHelp {
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
    pub(crate) fn settings_base(&self) -> &Self {
        match self {
            Self::Form(form) => form.back.as_deref().map_or(self, Self::settings_base),
            _ => self,
        }
    }

    pub(crate) fn is_settings_root(&self) -> bool {
        match self {
            Self::Settings { .. } => true,
            Self::Mods(view) => view.remove.is_none() && view.install.is_none(),
            Self::Services(view) => view.remove.is_none(),
            _ => false,
        }
    }

    pub(super) fn opening_action(&self) -> Option<crate::input::Action> {
        use crate::input::Action;
        match self {
            Self::Commands { menu, .. } => menu.opening_action(),
            Self::Form(_) | Self::ScanFolders(_) | Self::QuickAccess { .. } | Self::ArtworkSearch => None,
            Self::Lyrics(_) => Some(Action::Lyrics),
            Self::Downloads(_) => Some(Action::Downloads),
            Self::Services(_) => Some(Action::Services),
            Self::Mods(_) => Some(Action::Mods),
            Self::Tags(_) => None,
            Self::Statistics => Some(Action::Statistics),
            Self::PlaybackTuner { .. } => Some(Action::PlaybackTuner),
            Self::Equalizer { .. } => Some(Action::Equalizer),
            Self::Panels { .. } => Some(Action::Panels),
            Self::Help { .. } | Self::SettingsHelp { .. } | Self::CopyableHelp { .. } => {
                Some(Action::Help)
            }
            Self::Settings { .. } => Some(Action::Settings),
            Self::Browser(browser) if browser.palette_file.is_some() => None,
            Self::Browser(browser) if browser.playlist_name.is_some() => None,
            Self::Browser(browser) if browser.download_form.is_some() => None,
            Self::Browser(browser) => Some(if browser.folder {
                Action::AddFolder
            } else {
                Action::AddFiles
            }),
            Self::Metadata { .. } => Some(Action::Metadata),
            Self::Folders { .. } => Some(Action::PlaylistFolders),
            Self::ComposePlaylists { .. } | Self::ComposeTracks { .. } => {
                Some(Action::ComposePlaylists)
            }
            Self::Transfer(_) | Self::MoveFiles(_) => Some(Action::Transfer),
            Self::FileMoveReport { .. } => None,
            Self::RemoveEntry { .. }
            | Self::RemoveGroup { .. }
            | Self::RemovePlaylist { .. }
            | Self::ConfirmSettings { .. } => Some(Action::Delete),
            Self::Text(_)
            | Self::CaptureBinding { .. }
            | Self::ConfirmResetBindings { .. } => None,
            Self::Search(_) => None,
        }
    }

    pub(crate) fn is_settings(&self) -> bool {
        if matches!(self, Self::Lyrics(view) if view.from_settings) { return true; }
        if let Self::Form(form) = self {
            return form.back.as_deref().is_some_and(Self::is_settings);
        }
        matches!(self, Self::Browser(browser) if browser.palette_file.is_some())
            || matches!(
                self,
                Self::Settings { .. }
                    | Self::Mods(_)
                    | Self::Services(_)
                    | Self::Text(TextDialog {
                        purpose: TextPurpose::Settings(_),
                        ..
                    })
                    | Self::ConfirmSettings { .. }
                    | Self::ConfirmResetBindings { .. }
                    | Self::SettingsHelp { .. }
                    | Self::CopyableHelp { .. }
                    | Self::CaptureBinding { .. }
            )
    }
}

impl App {
    pub fn copy_help(&mut self, line: Option<usize>) -> bool {
        let description = match &self.view.dialog {
            Some(Dialog::CopyableHelp { description, .. } | Dialog::FileMoveReport { description, .. }) => description.as_str(),
            Some(Dialog::Settings { .. }) => match self.settings_guide() {
                Some(text) => text,
                None => return false,
            },
            _ => return false,
        };
        let text = match line {
            Some(line) => {
                let Some(text) = description.lines().nth(line) else {
                    return false;
                };
                text.to_owned()
            }
            None => description.to_owned(),
        };
        self.view.clipboard_text = Some(text);
        true
    }

    pub fn clipboard_result(&mut self, result: std::result::Result<(), String>, terminal: bool) {
        match result {
            Ok(()) => self.message(
                if terminal {
                    self.text(
                        "Copy request sent to the terminal",
                        "Запрос копирования передан терминалу",
                    )
                } else {
                    self.text("Copied", "Скопировано")
                }
                .into(),
            ),
            Err(error) => self.failure(anyhow::anyhow!(
                "{}: {error}",
                self.text("Could not copy text", "Не удалось скопировать текст")
            )),
        }
    }

    pub(super) fn text_dialog(&mut self, purpose: TextPurpose, text: String) {
        let selected_all = !text.is_empty();
        let color_picker = if matches!(
            purpose,
            TextPurpose::Settings(SettingsEdit::PaletteColor { .. })
        ) {
            crate::preferences::parse_color(&text)
                .ok()
                .map(crate::color_picker::ColorPicker::new)
        } else {
            None
        };
        self.view.dialog = Some(Dialog::Text(TextDialog {
            purpose,
            text,
            keyboard: self.settings.preferences.language,
            upper: false,
            selected_all,
            color_picker,
        }));
    }

    pub(super) fn scroll_dialog(&mut self, direction: i16) {
        if matches!(self.view.dialog, Some(Dialog::Help { .. })) && self.view.help_search.active() {
            self.move_help_match(i64::from(direction));
            return;
        }
        if self.view.dialog.as_ref().is_some_and(Dialog::is_settings_root) {
            self.settings_scroll(i64::from(direction));
            return;
        }
        if matches!(self.view.dialog, Some(Dialog::ScanFolders(_))) {
            self.scan_folders_scroll(i64::from(direction));
            return;
        }
        if matches!(self.view.dialog,Some(Dialog::Mods(_))) { self.mods_scroll(i64::from(direction));return; }
        if matches!(self.view.dialog,Some(Dialog::Services(_))) { self.services_scroll(i64::from(direction));return; }
        if matches!(self.view.dialog,Some(Dialog::Downloads(_))) { self.downloads_scroll(i64::from(direction));return; }
        if matches!(self.view.dialog,Some(Dialog::Lyrics(_))) { self.lyrics_scroll(i64::from(direction));return; }
        if matches!(self.view.dialog, Some(Dialog::Tags(_))) {
            self.tags_scroll(i64::from(direction));
            return;
        }
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
            self.view.pointer = ratatui::layout::Position::new(u16::MAX, u16::MAX);
            return;
        }
        let transfer_length = match &self.view.dialog {
            Some(Dialog::Transfer(view)) => self.transfer_destinations(view.source).len(),
            _ => 0,
        };
        let compose_length = self.compose_choices().len();
        let folder_length = if let Some(Dialog::Folders { playlist, .. }) = &self.view.dialog {
            self.library
                .playlists
                .iter()
                .find(|item| item.id == *playlist)
                .map_or(0, |item| item.scan_folders().len())
        } else {
            0
        };
        if matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
            self.settings_scroll(i64::from(direction));
            return;
        }
        match &mut self.view.dialog {
            Some(Dialog::QuickAccess { selected }) => {
                *selected = bounded(*selected, i64::from(direction), crate::toolbar::QuickButton::ALL.len());
            }
            Some(Dialog::ComposeTracks {
                selected, entries, ..
            }) => *selected = bounded(*selected, i64::from(direction), entries.len()),
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
                browser.scroll_selected = None;
                browser.selected =
                    bounded(browser.selected, direction as i64, browser.entries.len());
                browser.selection_explicit = !browser.entries.is_empty();
            }
            Some(Dialog::CaptureBinding { .. }) => {
                self.view.settings.binding_offset = bounded(
                    self.view.settings.binding_offset,
                    i64::from(direction) * 8,
                    crate::input::Key::shortcut_choices().count(),
                );
            }
            Some(Dialog::Transfer(view)) => {
                if view.confirm.is_some() {
                    view.offset = bounded(view.offset, direction as i64, self.view.dialog_scroll_max + 1);
                } else { view.selected = bounded(view.selected, direction as i64, transfer_length); }
            }
            Some(Dialog::MoveFiles(view)) if !view.confirm => {
                view.selected = bounded(view.selected, direction as i64, view.folders.len());
            }
            Some(Dialog::Help { offset })
            | Some(Dialog::Metadata { offset, .. })
            | Some(Dialog::CopyableHelp { offset, .. })
            | Some(Dialog::FileMoveReport { offset, .. })
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

    pub fn transfer_destinations(&self, source: i64) -> Vec<&Playlist> {
        self.library
            .playlists
            .iter()
            .filter(|playlist| {
                playlist.id != source
            })
            .collect()
    }

    pub(super) fn submit(&mut self) -> Result<()> {
        if matches!(self.view.dialog, Some(Dialog::MoveFiles(_))) {
            return self.submit_move_files();
        }
        if matches!(self.view.dialog, Some(Dialog::Transfer(_))) { return self.submit_transfer(); }
        if let Some(Dialog::CaptureBinding { index }) = self.view.dialog.as_ref()
            && let Some(selected) = self.view.settings.binding_global_focus
        {
            return self.target(
                if selected == 0 {
                    super::Target::BindingGlobal(*index)
                } else {
                    super::Target::BindingGlobalEdit(*index)
                },
                false,
            );
        }
        if let Some(Dialog::Commands { selected, .. }) = &self.view.dialog {
            return self.activate_command(*selected);
        }
        if matches!(self.view.dialog, Some(Dialog::Search(_))) {
            return self.play_search_result();
        }
        match self.view.dialog.take() {
            Some(Dialog::ComposePlaylists { selected, ids }) => {
                if ids.is_empty() {
                    self.view.dialog = Some(Dialog::ComposePlaylists { selected, ids });
                    anyhow::bail!(
                        "{}",
                        self.text("Select a source playlist", "Отметьте исходный плейлист.")
                    );
                }
                self.open_composition_tracks(ids);
            }
            Some(Dialog::ComposeTracks {
                sources,
                selected,
                entries,
                included,
            }) => {
                let chosen: Vec<_> = entries
                    .iter()
                    .copied()
                    .filter(|id| included.contains(id))
                    .collect();
                if chosen.is_empty() {
                    self.view.dialog = Some(Dialog::ComposeTracks {
                        sources,
                        selected,
                        entries,
                        included,
                    });
                    anyhow::bail!(
                        "{}",
                        self.text(
                            "Select at least one track",
                            "Отметьте хотя бы одну композицию"
                        )
                    );
                }
                let name = self.fresh_playlist_name(self.text("Mix", "Подборка"));
                self.text_dialog(
                    TextPurpose::ComposeSelection {
                        sources,
                        entries: chosen,
                        available: entries,
                    },
                    name,
                );
            }
            Some(Dialog::QuickAccess { selected }) => {
                self.view.dialog = Some(Dialog::QuickAccess { selected });
                self.quick_access_action(crate::toolbar::QuickAction::Toggle(selected))?;
            }
            Some(Dialog::Panels { selected, expanded }) => {
                self.view.dialog = Some(Dialog::Panels { selected, expanded });
                self.activate_panel_row(selected, false)?;
            }
            Some(Dialog::Text(dialog)) => {
                let recovery = Some(Dialog::Text(dialog.clone()));
                let text = dialog.text.clone();
                let mode = self.settings.mode;
                let result = match &dialog.purpose {
                    TextPurpose::Create => self.start_database(recovery, move |store| {
                        Ok(DatabaseOutcome::Created(
                            store.create_playlist(&text, mode)?,
                        ))
                    }),
                    TextPurpose::CreateWithFolder { parent } => {
                        let parent = parent.clone();
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Created(
                                store.create_playlist_with_folder(&text, mode, &parent)?,
                            ))
                        })
                    }
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
                                store.compose_playlists(&ids, &text, mode, true)?,
                            ))
                        })
                    }
                    TextPurpose::ComposeSelection {
                        sources, entries, ..
                    } => {
                        let sources = sources.clone();
                        let entries = entries.clone();
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Created(
                                store.compose_entries(&sources, &entries, &text, mode)?,
                            ))
                        })
                    }
                    TextPurpose::RenamePlaylist(id) => {
                        let id = *id;
                        let unlocked = self.playlist_unlocked(id);
                        self.start_database(recovery, move |store| {
                            Ok(DatabaseOutcome::Changed(
                                store.rename_playlist(id, &text, unlocked)?,
                            ))
                        })
                    }
                    TextPurpose::PlaylistFolder(id, previous) => {
                        let id = *id;
                        let unlocked = self.playlist_unlocked(id);
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
                        let unlocked = self.playlist_unlocked(id);
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
                        let unlocked = self.playlist_unlocked(playlist);
                        self.start_database(recovery, move |store| {
                            store.rename_track(track, &text, playlist, unlocked)?;
                            Ok(DatabaseOutcome::Renamed)
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
                            self.settings.preferences.themes[self.settings.mode.index()].accent =
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
            Some(Dialog::RemovePlaylist {
                playlist,
                name,
                confirm,
            }) => {
                if !confirm {
                    return Ok(());
                }
                let recovery = Dialog::RemovePlaylist {
                    playlist,
                    name: name.clone(),
                    confirm: false,
                };
                let unlocked = self.playlist_unlocked(playlist);
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.delete_playlist(playlist, &name, unlocked)?,
                    ))
                }) {
                    self.view.dialog = Some(recovery);
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
                let unlocked = self.playlist_unlocked(playlist);
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(
                        store.remove_entry(playlist, entry, unlocked)?,
                    ))
                }) {
                    self.view.dialog = Some(recovery);
                    return Err(error);
                }
            }
            Some(Dialog::RemoveGroup { playlist, folder, name, entries }) => {
                let recovery = Dialog::RemoveGroup { playlist, folder: folder.clone(), name, entries: entries.clone() };
                let unlocked = self.playlist_unlocked(playlist);
                if let Err(error) = self.start_database(Some(recovery.clone()), move |store| {
                    Ok(DatabaseOutcome::Changed(store.remove_group(playlist, &folder, &entries, unlocked)?))
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
            Some(Dialog::ConfirmResetBindings { confirm }) => {
                self.show_settings();
                if confirm {
                    self.settings.preferences.bindings = crate::input::default_bindings();
                    self.settings.preferences.global_shortcuts.clear();
                    self.save_settings()?;
                }
            }
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
