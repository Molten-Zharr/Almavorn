use super::{App, Dialog, Focus, TextPurpose, database::DatabaseOutcome};
use crate::{
    input::Action,
    model::{Mode, PlaylistKind},
};
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

impl App {
    pub fn allowed(&self, action: Action) -> bool {
        use Action::*;
        if self.database_busy()
            && matches!(
                action,
                AddFiles
                    | AddFolder
                    | AddNew
                    | NewPlaylist
                    | Rename
                    | Delete
                    | MoveUp
                    | MoveDown
                    | Transfer
                    | Undo
                    | Redo
                    | ToggleEdit
                    | ToggleDesk
                    | SwitchMode
            )
        {
            return false;
        }
        if self.importing()
            && matches!(
                action,
                AddFiles | AddFolder | AddNew | ToggleEdit | ToggleDesk | SwitchMode
            )
        {
            return false;
        }
        let editable = self
            .playlist()
            .is_some_and(|playlist| playlist.can_edit(self.editing));
        match action {
            ToggleEdit => self.settings.mode == Mode::Order,
            Rename | Delete => {
                editable
                    && match self.focus {
                        Focus::Playlists => self
                            .playlist()
                            .is_some_and(|playlist| playlist.kind == PlaylistKind::Normal),
                        Focus::Tracks => self.entry().is_some_and(|entry| {
                            action == Delete
                                || self.editing
                                || !self.playlists.iter().any(|playlist| {
                                    playlist.mode == Mode::Order
                                        && playlist.kind == PlaylistKind::Normal
                                        && playlist
                                            .entries
                                            .iter()
                                            .any(|other| other.track.id == entry.track.id)
                                })
                        }),
                    }
            }
            MoveUp | MoveDown => {
                if !editable {
                    return false;
                }
                let (index, length) = if self.focus == Focus::Playlists {
                    let lists: Vec<_> = self
                        .visible_playlists()
                        .into_iter()
                        .filter(|playlist| playlist.kind == PlaylistKind::Normal)
                        .collect();
                    (
                        lists
                            .iter()
                            .position(|playlist| Some(playlist.id) == self.selected_playlist),
                        lists.len(),
                    )
                } else {
                    if self.sort != crate::app::Sort::Position || !self.query.is_empty() {
                        return false;
                    }
                    self.playlist()
                        .map(|playlist| {
                            (
                                playlist
                                    .entries
                                    .iter()
                                    .position(|entry| Some(entry.id) == self.selected_entry),
                                playlist.entries.len(),
                            )
                        })
                        .unwrap_or((None, 0))
                };
                index.is_some_and(|index| {
                    if action == MoveUp {
                        index > 0
                    } else {
                        index + 1 < length
                    }
                })
            }
            TogglePlay => {
                self.current.is_some() || self.preparing_playback() || self.entry().is_some()
            }
            Next => self.can_step_playback(1),
            Previous => self.can_step_playback(-1),
            Stop => self.current.is_some() || self.preparing_playback(),
            SeekForward | SeekBackward => self.current.is_some(),
            VolumeUp => self.settings.volume < 1.0,
            VolumeDown => self.settings.volume > 0.0,
            Transfer | Metadata | Mark => self.entry().is_some(),
            AddFiles | AddFolder => self.import_target().is_some(),
            AddNew => {
                self.import_target().is_some()
                    && self
                        .playlist()
                        .is_some_and(|playlist| !playlist.entries.is_empty())
            }
            Undo | Redo => self.scope().is_some_and(|scope| {
                self.histories.get(scope).is_some_and(|(undo, redo)| {
                    if action == Undo {
                        !undo.is_empty()
                    } else {
                        !redo.is_empty()
                    }
                })
            }),
            _ => true,
        }
    }

    pub fn action(&mut self, action: Action) -> Result<()> {
        if !self.allowed(action) {
            self.message(
                self.text(
                    "Action unavailable. Check selection and Order editing.",
                    "Действие недоступно. Проверьте выбор и редактирование Порядка.",
                )
                .into(),
            );
            return Ok(());
        }
        use Action::*;
        match action {
            Quit => self.quit = true,
            Help => self.dialog = Some(Dialog::Help { offset: 0 }),
            Settings => self.open_settings_page(super::SettingsPage::General),
            TogglePlay => self.toggle_playback()?,
            Stop => {
                self.stop_playback();
            }
            Next => self.next(1, true)?,
            Previous => self.next(-1, true)?,
            VolumeUp | VolumeDown => {
                self.adjust_volume(if action == VolumeUp { 5 } else { -5 })?;
            }
            SeekForward | SeekBackward => {
                if let Some(audio) = &self.audio {
                    let position = audio.position();
                    audio.seek(if action == SeekForward {
                        position.saturating_add(Duration::from_secs(5))
                    } else {
                        position.saturating_sub(Duration::from_secs(5))
                    })?;
                }
            }
            SwitchMode => self.set_mode(if self.settings.mode == Mode::Order {
                Mode::Chaos
            } else {
                Mode::Order
            })?,
            ToggleEdit => {
                self.editing = !self.editing;
                self.message(
                    self.text("Order editing changed", "Редактирование Порядка изменено")
                        .into(),
                );
            }
            ToggleDesk => {
                self.settings.sorting_desk = !self.settings.sorting_desk;
                self.save_settings()?;
            }
            AddFiles | AddFolder => {
                let directory = dirs::audio_dir()
                    .or_else(dirs::home_dir)
                    .unwrap_or(std::env::current_dir()?);
                self.show_browser(directory, action == AddFolder);
            }
            AddNew => {
                let directories: HashSet<PathBuf> = self
                    .playlist()
                    .into_iter()
                    .flat_map(|playlist| &playlist.entries)
                    .filter_map(|entry| entry.track.path.parent().map(Path::to_path_buf))
                    .collect();
                let mut directories: Vec<_> = directories.into_iter().collect();
                directories.sort();
                self.start_import(directories)?;
            }
            NewPlaylist => {
                let mut number = 1;
                let name = loop {
                    let name = format!("{} {number}", self.text("Playlist", "Плейлист"));
                    if !self.playlists.iter().any(|playlist| {
                        playlist.mode == self.settings.mode
                            && playlist.name.to_lowercase() == name.to_lowercase()
                    }) {
                        break name;
                    }
                    number += 1;
                };
                self.text_dialog(TextPurpose::Create, name);
            }
            Rename => {
                if self.focus == Focus::Playlists {
                    if let Some(playlist) = self.playlist() {
                        self.text_dialog(
                            TextPurpose::RenamePlaylist(playlist.id),
                            playlist.name.clone(),
                        );
                    }
                } else if let (Some(playlist), Some(entry)) = (self.playlist(), self.entry()) {
                    self.text_dialog(
                        TextPurpose::RenameTrack(entry.track.id, playlist.id),
                        entry.track.title.clone(),
                    );
                }
            }
            Delete => {
                if self.focus == Focus::Playlists {
                    if let Some(playlist) = self.playlist() {
                        self.text_dialog(
                            TextPurpose::DeletePlaylist(playlist.id, playlist.name.clone()),
                            String::new(),
                        );
                    }
                } else if let (Some(playlist), Some(entry)) = (self.playlist(), self.entry()) {
                    self.dialog = Some(Dialog::RemoveEntry {
                        playlist: playlist.id,
                        entry: entry.id,
                        title: entry.track.title.clone(),
                    });
                }
            }
            MoveUp | MoveDown => {
                let direction = if action == MoveUp { -1 } else { 1 };
                if let Some(playlist) = self.selected_playlist {
                    let focus = self.focus;
                    let entry = self.selected_entry;
                    let unlocked = self.editing;
                    self.start_database(None, move |store| {
                        let change = if focus == Focus::Playlists {
                            store.move_playlist(playlist, direction, unlocked)?
                        } else {
                            store.move_entry(
                                playlist,
                                entry.context("Select a track first")?,
                                direction,
                                unlocked,
                            )?
                        };
                        Ok(DatabaseOutcome::Changed(change))
                    })?;
                }
            }
            Transfer => {
                let ids = self
                    .playlist()
                    .into_iter()
                    .flat_map(|playlist| &playlist.entries)
                    .filter(|entry| {
                        self.marked.contains(&entry.id)
                            || (self.marked.is_empty() && Some(entry.id) == self.selected_entry)
                    })
                    .map(|entry| entry.track.id)
                    .collect();
                self.dialog = Some(Dialog::Transfer { ids, selected: 0 });
            }
            Search => self.text_dialog(TextPurpose::Search, self.query.clone()),
            Sort => {
                self.sort = self.sort.next();
                self.track_offset = 0;
                self.refresh()?;
            }
            Undo | Redo => {
                if let Some(scope) = self.scope() {
                    let change = self.histories.get_mut(scope).and_then(|(undo, redo)| {
                        if action == Undo {
                            undo.pop()
                        } else {
                            redo.pop()
                        }
                    });
                    if let Some(change) = change {
                        let redo = action == Redo;
                        let recovery = change.clone();
                        let result = self.start_database(None, move |store| {
                            let (expected, replacement) = if redo {
                                (&change.before, &change.after)
                            } else {
                                (&change.after, &change.before)
                            };
                            store.restore(scope, expected, replacement)?;
                            Ok(DatabaseOutcome::Restored { change, redo })
                        });
                        match result {
                            Ok(()) => {
                                self.database
                                    .as_mut()
                                    .expect("database request is present")
                                    .undo = Some((recovery, redo))
                            }
                            Err(error) => {
                                let history = self.histories.entry(scope).or_default();
                                if redo {
                                    history.1.push(recovery);
                                } else {
                                    history.0.push(recovery);
                                }
                                return Err(error);
                            }
                        }
                    }
                }
            }
            Metadata => {
                if let Some(entry) = self.entry() {
                    self.dialog = Some(Dialog::Metadata {
                        track: entry.track.clone(),
                        offset: 0,
                    });
                }
            }
            Mark => {
                if let Some(id) = self.selected_entry
                    && !self.marked.insert(id)
                {
                    self.marked.remove(&id);
                }
            }
            PlaylistPanel => {
                self.settings.playlist_placement = self.settings.playlist_placement.next();
                self.settings.workspace.root = None;
                self.save_settings()?;
            }
            PlayerPanel => {
                self.settings.player_placement = self.settings.player_placement.next();
                self.settings.workspace.root = None;
                self.save_settings()?;
            }
            Panels => self.dialog = Some(Dialog::Panels { selected: 0 }),
        }
        Ok(())
    }
}
