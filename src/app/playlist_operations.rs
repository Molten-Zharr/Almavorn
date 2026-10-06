use super::{App, Dialog, TextPurpose};
use crate::errors::AppError;
use anyhow::{Context, Result, ensure};

impl App {
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
