use super::{App, Dialog, Target, TextPurpose};
use crate::errors::AppError;
use crate::input::{Key, KeyPress};
use anyhow::{Context, Result, ensure};

impl App {
    pub(super) fn composition_key(&mut self, key: KeyPress) -> Result<()> {
        let selected = match &self.view.dialog {
            Some(
                Dialog::ComposePlaylists { selected, .. } | Dialog::ComposeTracks { selected, .. },
            ) => *selected,
            _ => return Ok(()),
        };
        if key.ctrl && !key.alt && key.key == Key::Char('a') {
            self.composition_all(!key.shift);
            return Ok(());
        }
        if !key.ctrl && !key.alt && key.key == Key::Backspace {
            self.composition_back();
            return Ok(());
        }
        if key.ctrl && !key.alt && matches!(key.key, Key::Up | Key::Down) {
            self.move_composition_source(if key.key == Key::Up { -1 } else { 1 });
            return Ok(());
        }
        if !key.ctrl && !key.alt && key.key == Key::Char(' ') {
            self.toggle_compose_row(selected);
            return Ok(());
        }
        if matches!(key.key, Key::Home | Key::End) {
            self.scroll_dialog(if key.key == Key::Home {
                i16::MIN
            } else {
                i16::MAX
            });
            return Ok(());
        }
        self.dialog_navigation_key(key)
    }

    pub(super) fn folders_key(&mut self, key: KeyPress) -> Result<()> {
        if !key.ctrl && !key.alt {
            let target = match key.key {
                Key::Insert => Some(Target::FolderAdd),
                Key::F(3) | Key::Enter => Some(Target::FolderEdit),
                Key::Delete => Some(Target::FolderRemove),
                Key::Char('r') => Some(Target::FolderScan),
                Key::Char(' ') => Some(Target::ScanSubfolders),
                _ => None,
            };
            if let Some(target) = target {
                return self.target(target, false);
            }
        }
        self.dialog_navigation_key(key)
    }

    pub(crate) fn compose_choices(&self) -> Vec<&crate::model::Playlist> {
        self.library.playlists.iter().collect()
    }

    pub(super) fn fresh_playlist_name(&self, base: &str) -> String {
        let base = base.chars().take(145).collect::<String>();
        for number in 0.. {
            let name = if number == 0 {
                base.clone()
            } else {
                format!("{base} ({number})")
            };
            if !self.library.playlists.iter().any(|item| {
                item.mode == self.settings.mode && item.name.to_lowercase() == name.to_lowercase()
            }) {
                return name;
            }
        }
        unreachable!()
    }

    pub(super) fn toggle_compose_row(&mut self, row: usize) {
        if let Some(Dialog::ComposeTracks {
            selected,
            entries,
            included,
            ..
        }) = &mut self.view.dialog
        {
            if let Some(id) = entries.get(row) {
                *selected = row;
                if !included.remove(id) {
                    included.insert(*id);
                }
            }
            return;
        }
        let id = self.compose_choices().get(row).map(|item| item.id);
        if let Some(id) = id
            && let Some(Dialog::ComposePlaylists { selected, ids }) = &mut self.view.dialog
        {
            *selected = row;
            if let Some(index) = ids.iter().position(|value| *value == id) {
                ids.remove(index);
            } else {
                ids.push(id);
            }
        }
    }

    pub(super) fn move_composition_source(&mut self, direction: i64) {
        if let Some(Dialog::ComposeTracks {
            selected, entries, ..
        }) = &mut self.view.dialog
        {
            if !entries.is_empty() {
                let target = super::bounded(*selected, direction, entries.len());
                entries.swap(*selected, target);
                *selected = target;
            }
            return;
        }
        let id = if let Some(Dialog::ComposePlaylists { selected, .. }) = &self.view.dialog {
            self.compose_choices().get(*selected).map(|item| item.id)
        } else {
            None
        };
        if let Some(id) = id
            && let Some(Dialog::ComposePlaylists { ids, .. }) = &mut self.view.dialog
            && let Some(index) = ids.iter().position(|value| *value == id)
        {
            let target = super::bounded(index, direction, ids.len());
            ids.swap(index, target);
        }
    }

    pub(super) fn open_composition_tracks(&mut self, sources: Vec<i64>) {
        let mut seen = std::collections::HashSet::new();
        let entries: Vec<_> = sources
            .iter()
            .flat_map(|id| {
                self.library
                    .playlists
                    .iter()
                    .find(|p| p.id == *id)
                    .into_iter()
                    .flat_map(|p| &p.entries)
            })
            .filter(|entry| seen.insert(entry.track.id))
            .map(|entry| entry.id)
            .collect();
        let included = entries.iter().copied().collect();
        self.view.dialog = Some(Dialog::ComposeTracks {
            sources,
            selected: 0,
            entries,
            included,
        });
    }

    pub(super) fn composition_all(&mut self, all: bool) {
        let choices = self.compose_choices().iter().map(|p| p.id).collect();
        match &mut self.view.dialog {
            Some(Dialog::ComposeTracks {
                entries, included, ..
            }) => {
                *included = if all {
                    entries.iter().copied().collect()
                } else {
                    Default::default()
                };
            }
            Some(Dialog::ComposePlaylists { ids, .. }) => {
                *ids = if all { choices } else { Vec::new() }
            }
            _ => {}
        }
    }

    pub(super) fn composition_back(&mut self) {
        if matches!(self.view.dialog, Some(Dialog::ComposeTracks { .. }))
            && let Some(Dialog::ComposeTracks { sources, .. }) = self.view.dialog.take()
        {
            self.view.dialog = Some(Dialog::ComposePlaylists {
                selected: 0,
                ids: sources,
            });
        }
    }

    pub(super) fn folder_selection(&self) -> Result<(i64, Option<String>)> {
        let Some(Dialog::Folders { playlist, selected }) = self.view.dialog.as_ref() else {
            anyhow::bail!(AppError::FolderMissing);
        };
        let item = self
            .library
            .playlists
            .iter()
            .find(|item| item.id == *playlist)
            .context(AppError::PlaylistMissing)?;
        Ok((
            *playlist,
            item.folders
                .get(*selected)
                .map(|path| path.to_string_lossy().into_owned()),
        ))
    }

    fn folder_editable(&self, id: i64) -> Result<()> {
        ensure!(!self.busy(), AppError::LibraryBusy);
        let playlist = self
            .library
            .playlists
            .iter()
            .find(|playlist| playlist.id == id)
            .context(AppError::PlaylistMissing)?;
        ensure!(self.playlist_editable(playlist), AppError::OrderProtected);
        Ok(())
    }

    pub(super) fn edit_playlist_folder(&mut self, replace: bool) -> Result<()> {
        let (id, folder) = self.folder_selection()?;
        self.folder_editable(id)?;
        let previous = if replace {
            Some(folder.context(AppError::FolderMissing)?)
        } else {
            None
        };
        let text = previous.clone().unwrap_or_else(|| {
            dirs::audio_dir()
                .or_else(dirs::home_dir)
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        self.text_dialog(TextPurpose::PlaylistFolder(id, previous), text);
        Ok(())
    }

    pub(super) fn request_folder_removal(&mut self) -> Result<()> {
        let (id, folder) = self.folder_selection()?;
        self.folder_editable(id)?;
        self.text_dialog(
            TextPurpose::RemovePlaylistFolder(id, folder.context(AppError::FolderMissing)?),
            String::new(),
        );
        Ok(())
    }
}
