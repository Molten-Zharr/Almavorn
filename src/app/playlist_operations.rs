use super::{App, Dialog, TextPurpose};
use crate::errors::AppError;
use anyhow::{Context, Result, ensure};

impl App {
    pub(crate) fn compose_choices(&self) -> Vec<&crate::model::Playlist> {
        self.visible_playlists()
            .into_iter()
            .filter(|item| item.can_edit(self.view.editing))
            .collect()
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
        ensure!(
            playlist.can_edit(self.view.editing),
            AppError::OrderProtected
        );
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
