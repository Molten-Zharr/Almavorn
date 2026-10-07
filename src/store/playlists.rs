use super::queries::playlist_access;
use super::{Change, Store, append_track, lock_ordering, playlist_name, unique_playlist_name};
use crate::{errors::AppError, model::Mode};
use anyhow::{Context, Result, ensure};
use rusqlite::params;
use std::path::Path;

impl Store {
    pub fn compose_playlists(
        &mut self,
        sources: &[i64],
        name: &str,
        mode: Mode,
        unlocked: bool,
    ) -> Result<Change> {
        ensure!(!sources.is_empty(), AppError::PlaylistSelectionRequired);
        ensure!(mode != Mode::Order || unlocked, AppError::OrderProtected);
        let name = playlist_name(name)?;
        self.mutate(None, mode, true, |connection| {
            let mut group_folders = false;
            for &source in sources {
                let (mode, kind) = playlist_access(connection, source)?;
                ensure!(mode != Mode::Order || kind == crate::model::PlaylistKind::SortingDesk || unlocked, AppError::OrderProtected);
                group_folders |= connection.query_row("SELECT group_folders FROM playlists WHERE id=?1", [source], |row| row.get::<_, bool>(0))?;
            }
            unique_playlist_name(connection, mode.key(), name, None)?;
            lock_ordering(connection, mode.key())?;
            connection.execute("INSERT INTO playlists(name,name_fold,mode,kind,position,group_folders) VALUES (?1,?2,?3,'normal',(SELECT COALESCE(MAX(position),-1)+1 FROM playlists WHERE mode=?3),?4)", params![name, name.to_lowercase(), mode.key(), group_folders])?;
            let destination = connection.last_insert_rowid();
            for &source in sources {
                let mut statement = connection.prepare("SELECT track_id,position FROM entries WHERE playlist_id=?1 ORDER BY position,id")?;
                let entries = statement.query_map([source], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                for (track, position) in entries {
                    if sources.len() == 1 {
                        connection.execute("UPDATE tracks SET revision=revision+1 WHERE id=?1", [track])?;
                        connection.execute("INSERT INTO entries(playlist_id,track_id,position) VALUES (?1,?2,?3)", params![destination, track, position])?;
                    } else {
                        append_track(connection, destination, track)?;
                    }
                }
                connection.execute("INSERT INTO playlist_folders(playlist_id,path) SELECT ?1,path FROM playlist_folders WHERE playlist_id=?2 ON CONFLICT DO NOTHING", params![destination, source])?;
            }
            Ok(())
        })
    }

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
