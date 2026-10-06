use super::{SavedEntry, SavedPlaylist, Snapshot};
use crate::errors::AppError;
use crate::model::{Entry, Mode, Playlist, PlaylistKind, Track};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension};
use std::{collections::HashMap, path::PathBuf};

fn mode(value: &str) -> Result<Mode> {
    match value {
        "order" => Ok(Mode::Order),
        "chaos" => Ok(Mode::Chaos),
        _ => bail!("Unknown playlist mode"),
    }
}

fn kind(value: &str) -> Result<PlaylistKind> {
    match value {
        "normal" => Ok(PlaylistKind::Normal),
        "desk" => Ok(PlaylistKind::SortingDesk),
        _ => bail!("Unknown playlist type"),
    }
}

pub(super) fn playlist_access(connection: &Connection, id: i64) -> Result<(Mode, PlaylistKind)> {
    let (saved_mode, saved_kind): (String, String) = connection
        .query_row("SELECT mode,kind FROM playlists WHERE id=?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .optional()?
        .context(AppError::PlaylistMissing)?;
    Ok((mode(&saved_mode)?, kind(&saved_kind)?))
}

pub(super) fn read_playlists(connection: &Connection) -> Result<Vec<Playlist>> {
    let mut statement = connection
        .prepare("SELECT id,name,mode,kind,position FROM playlists ORDER BY mode,position,id")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;
    let mut playlists = Vec::new();
    let mut indices = HashMap::new();
    for row in rows {
        let (id, name, saved_mode, saved_kind, position) = row?;
        indices.insert(id, playlists.len());
        playlists.push(Playlist {
            id,
            name,
            mode: mode(&saved_mode)?,
            kind: kind(&saved_kind)?,
            position,
            entries: Vec::new(),
            folders: Vec::new(),
        });
    }
    // Fetch entries once for the entire library, including shared tracks.
    let mut statement = connection.prepare(
        "SELECT e.playlist_id,e.id,e.position,t.id,t.path,t.title,t.artist,t.album,t.duration_ms,t.tags FROM entries e JOIN tracks t ON t.id=e.track_id ORDER BY e.playlist_id,e.position,e.id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            Entry {
                id: row.get(1)?,
                position: row.get(2)?,
                track: Track {
                    id: row.get(3)?,
                    path: PathBuf::from(row.get::<_, String>(4)?),
                    title: row.get(5)?,
                    artist: row.get(6)?,
                    album: row.get(7)?,
                    duration_ms: row.get::<_, i64>(8)? as u64,
                    tags: row.get(9)?,
                },
            },
        ))
    })?;
    for row in rows {
        let (playlist_id, entry) = row?;
        let index = indices
            .get(&playlist_id)
            .context("Entry refers to a missing playlist")?;
        playlists[*index].entries.push(entry);
    }
    for (id, path) in read_folders(connection, None)? {
        let index = indices.get(&id).context(AppError::PlaylistMissing)?;
        playlists[*index].folders.push(path);
    }
    Ok(playlists)
}

pub(super) fn snapshot(connection: &Connection, scope: &str) -> Result<Snapshot> {
    let mut statement = connection.prepare(
        "SELECT p.id,p.name,p.mode,p.kind,p.position FROM playlists p WHERE (?1='desk' AND p.kind='desk') OR (?1!='desk' AND p.mode=?1 AND p.kind='normal') ORDER BY p.mode,p.position,p.id",
    )?;
    let rows = statement.query_map([scope], |row| {
        Ok(SavedPlaylist {
            id: row.get(0)?,
            name: row.get(1)?,
            mode: row.get(2)?,
            kind: row.get(3)?,
            position: row.get(4)?,
            entries: Vec::new(),
            folders: Vec::new(),
        })
    })?;
    let mut playlists = Vec::new();
    let mut indices = HashMap::new();
    for row in rows {
        let playlist = row?;
        indices.insert(playlist.id, playlists.len());
        playlists.push(playlist);
    }
    // Undo stores track references and order; audio metadata is never copied.
    let mut statement = connection.prepare(
        "SELECT e.playlist_id,e.id,e.track_id,e.position FROM entries e JOIN playlists p ON p.id=e.playlist_id WHERE (?1='desk' AND p.kind='desk') OR (?1!='desk' AND p.mode=?1 AND p.kind='normal') ORDER BY e.playlist_id,e.position,e.id",
    )?;
    let rows = statement.query_map([scope], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            SavedEntry {
                id: row.get(1)?,
                track_id: row.get(2)?,
                position: row.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (playlist_id, entry) = row?;
        let index = indices
            .get(&playlist_id)
            .context("Entry refers to a missing playlist")?;
        playlists[*index].entries.push(entry);
    }
    for (id, path) in read_folders(connection, Some(scope))? {
        let index = indices.get(&id).context(AppError::PlaylistMissing)?;
        playlists[*index].folders.push(path);
    }
    Ok(Snapshot { playlists })
}

fn read_folders(connection: &Connection, scope: Option<&str>) -> Result<Vec<(i64, PathBuf)>> {
    let mut statement = connection.prepare(
        "SELECT f.playlist_id,f.path FROM playlist_folders f JOIN playlists p ON p.id=f.playlist_id WHERE ?1 IS NULL OR (?1='desk' AND p.kind='desk') OR (?1!='desk' AND p.mode=?1 AND p.kind='normal') ORDER BY f.playlist_id,f.path",
    )?;
    Ok(statement
        .query_map([scope], |row| {
            Ok((row.get(0)?, PathBuf::from(row.get::<_, String>(1)?)))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}
