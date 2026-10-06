use super::{Change, Store};
use crate::{errors::AppError, model::Mode};
use anyhow::{Context, Result, ensure};
use rusqlite::params;
use std::path::Path;

impl Store {
    pub fn set_playlist_folder(
        &mut self,
        playlist: i64,
        previous: Option<&str>,
        folder: &Path,
        unlocked: bool,
    ) -> Result<Change> {
        let folder = folder.canonicalize().context(AppError::FolderNotReady)?;
        ensure!(folder.is_dir(), AppError::FolderNotReady);
        let path = folder.to_str().context(AppError::InvalidPathEncoding)?;
        self.mutate(Some(playlist), Mode::Order, unlocked, |connection| {
            let exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM playlist_folders WHERE playlist_id=?1 AND path=?2)",
                params![playlist, path],
                |row| row.get(0),
            )?;
            ensure!(
                !exists || previous == Some(path),
                AppError::FolderAlreadyRegistered
            );
            if let Some(previous) = previous {
                ensure!(
                    connection.execute(
                        "DELETE FROM playlist_folders WHERE playlist_id=?1 AND path=?2",
                        params![playlist, previous],
                    )? == 1,
                    AppError::FolderMissing
                );
            }
            connection.execute(
                "INSERT INTO playlist_folders(playlist_id,path) VALUES (?1,?2)",
                params![playlist, path],
            )?;
            Ok(())
        })
    }

    pub fn remove_playlist_folder(
        &mut self,
        playlist: i64,
        folder: &str,
        confirmation: &str,
        unlocked: bool,
    ) -> Result<Change> {
        ensure!(folder == confirmation, AppError::FolderConfirmationRequired);
        self.mutate(Some(playlist), Mode::Order, unlocked, |connection| {
            ensure!(
                connection.execute(
                    "DELETE FROM playlist_folders WHERE playlist_id=?1 AND path=?2",
                    params![playlist, folder],
                )? == 1,
                AppError::FolderMissing
            );
            Ok(())
        })
    }
}
