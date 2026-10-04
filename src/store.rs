use crate::model::{Entry, ImportedTrack, Mode, Playlist, PlaylistKind, Settings, Track};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedEntry {
    pub id: i64,
    pub track_id: i64,
    pub position: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedPlaylist {
    pub id: i64,
    pub name: String,
    pub mode: String,
    pub kind: String,
    pub position: i64,
    pub entries: Vec<SavedEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub playlists: Vec<SavedPlaylist>,
}

pub struct Change {
    pub scope: &'static str,
    pub before: Snapshot,
    pub after: Snapshot,
}

pub struct Store {
    connection: Connection,
    pub path: PathBuf,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory).context("Cannot create data directory")?;
        let path = directory.join("almavorn.sqlite3");
        let mut connection = Connection::open(&path).context("Cannot open music database")?;
        connection.busy_timeout(Duration::from_secs(3))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            version <= 1,
            "Database was created by a newer version of Almavorn"
        );
        if version == 0 {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current: i64 =
                transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
            ensure!(
                current <= 1,
                "Database was created by a newer version of Almavorn"
            );
            if current == 0 {
                transaction.execute_batch(
                    "CREATE TABLE tracks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT NOT NULL UNIQUE,
                    title TEXT NOT NULL, artist TEXT NOT NULL, album TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0), tags TEXT NOT NULL
                 );
                 CREATE TABLE playlists (
                    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL,
                    name_fold TEXT NOT NULL, mode TEXT NOT NULL CHECK(mode IN ('order','chaos')),
                    kind TEXT NOT NULL CHECK(kind IN ('normal','desk')),
                    position INTEGER NOT NULL, UNIQUE(mode, name_fold)
                 );
                 CREATE UNIQUE INDEX one_sorting_desk ON playlists(kind) WHERE kind='desk';
                 CREATE TABLE entries (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
                    track_id INTEGER NOT NULL REFERENCES tracks(id),
                    position INTEGER NOT NULL CHECK(position >= 0),
                    UNIQUE(playlist_id, position), UNIQUE(playlist_id, track_id)
                 );
                 CREATE INDEX entries_track ON entries(track_id);
                 CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO playlists(name,name_fold,mode,kind,position)
                 VALUES ('Sorting desk','sorting desk','order','desk',-1);
                 PRAGMA user_version=1;",
                )?;
            }
            transaction.commit()?;
        }
        Ok(Self { connection, path })
    }

    pub fn settings(&self) -> Result<Settings> {
        let json: Option<String> = self
            .connection
            .query_row("SELECT value FROM settings WHERE key='app'", [], |row| {
                row.get(0)
            })
            .optional()?;
        match json {
            Some(json) => serde_json::from_str(&json).context("Saved settings are damaged"),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        self.connection.execute("INSERT INTO settings(key,value) VALUES ('app',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(settings)?])?;
        Ok(())
    }

    pub fn data_version(&self) -> Result<i64> {
        Ok(self
            .connection
            .pragma_query_value(None, "data_version", |row| row.get(0))?)
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        read_playlists(&self.connection)
    }

    fn mutate<F>(
        &mut self,
        playlist_id: Option<i64>,
        mode: Mode,
        unlocked: bool,
        operation: F,
    ) -> Result<Change>
    where
        F: FnOnce(&Connection) -> Result<()>,
    {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = if let Some(id) = playlist_id {
            let playlist = read_playlists(&transaction)?
                .into_iter()
                .find(|playlist| playlist.id == id)
                .context("Playlist no longer exists")?;
            ensure!(
                playlist.can_edit(unlocked),
                "Enable Order editing before changing this playlist"
            );
            if playlist.kind == PlaylistKind::SortingDesk {
                "desk"
            } else {
                playlist.mode.key()
            }
        } else {
            mode.key()
        };
        let before = snapshot(&transaction, scope)?;
        operation(&transaction)?;
        let after = snapshot(&transaction, scope)?;
        transaction.commit()?;
        Ok(Change {
            scope,
            before,
            after,
        })
    }

    pub fn create_playlist(&mut self, name: &str, mode: Mode) -> Result<Change> {
        let name = playlist_name(name)?;
        self.mutate(None, mode, true, |connection| {
            connection.execute("INSERT INTO playlists(name,name_fold,mode,kind,position) VALUES (?1,?2,?3,'normal',(SELECT COALESCE(MAX(position),-1)+1 FROM playlists WHERE mode=?3))", params![name, name.to_lowercase(), mode.key()])?;
            Ok(())
        })
    }

    pub fn rename_playlist(&mut self, id: i64, name: &str, unlocked: bool) -> Result<Change> {
        let name = playlist_name(name)?;
        self.mutate(Some(id), Mode::Order, unlocked, |connection| {
            let kind: String =
                connection.query_row("SELECT kind FROM playlists WHERE id=?1", [id], |row| {
                    row.get(0)
                })?;
            ensure!(kind == "normal", "The sorting desk cannot be renamed");
            connection.execute(
                "UPDATE playlists SET name=?1,name_fold=?2 WHERE id=?3",
                params![name, name.to_lowercase(), id],
            )?;
            Ok(())
        })
    }

    pub fn delete_playlist(&mut self, id: i64, typed_name: &str, unlocked: bool) -> Result<Change> {
        self.mutate(Some(id), Mode::Order, unlocked, |connection| {
            let (name, kind): (String, String) = connection.query_row(
                "SELECT name,kind FROM playlists WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            ensure!(kind == "normal", "The sorting desk cannot be deleted");
            ensure!(
                name == typed_name,
                "Enter the exact playlist name to confirm removal"
            );
            connection.execute("DELETE FROM playlists WHERE id=?1", [id])?;
            Ok(())
        })
    }

    pub fn add_tracks(
        &mut self,
        playlist_id: i64,
        tracks: &[ImportedTrack],
        unlocked: bool,
    ) -> Result<Change> {
        self.mutate(Some(playlist_id), Mode::Order, unlocked, |connection| {
            for track in tracks {
                let path = track.path.to_str().context("File path is not valid Unicode")?;
                connection.execute("INSERT INTO tracks(path,title,artist,album,duration_ms,tags) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(path) DO NOTHING", params![path, track.title, track.artist, track.album, track.duration_ms.min(i64::MAX as u64) as i64, track.tags])?;
                let track_id: i64 = connection.query_row("SELECT id FROM tracks WHERE path=?1", [path], |row| row.get(0))?;
                append_track(connection, playlist_id, track_id)?;
            }
            Ok(())
        })
    }

    pub fn copy_tracks(
        &mut self,
        destination: i64,
        track_ids: &[i64],
        unlocked: bool,
    ) -> Result<Change> {
        self.mutate(Some(destination), Mode::Order, unlocked, |connection| {
            for &id in track_ids {
                append_track(connection, destination, id)?;
            }
            Ok(())
        })
    }

    pub fn remove_entry(&mut self, playlist: i64, entry: i64, unlocked: bool) -> Result<Change> {
        self.mutate(Some(playlist), Mode::Order, unlocked, |connection| {
            ensure!(
                connection.execute(
                    "DELETE FROM entries WHERE id=?1 AND playlist_id=?2",
                    params![entry, playlist]
                )? == 1,
                "Track no longer exists in this playlist"
            );
            // Метаданные композиции остаются в базе даже после удаления из списка.
            Ok(())
        })
    }

    pub fn move_entry(
        &mut self,
        playlist: i64,
        entry: i64,
        direction: i64,
        unlocked: bool,
    ) -> Result<Change> {
        self.mutate(Some(playlist), Mode::Order, unlocked, |connection| {
            let position: i64 = connection.query_row("SELECT position FROM entries WHERE id=?1 AND playlist_id=?2", params![entry, playlist], |row| row.get(0))?;
            let sql = if direction < 0 { "SELECT id,position FROM entries WHERE playlist_id=?1 AND position<?2 ORDER BY position DESC LIMIT 1" } else { "SELECT id,position FROM entries WHERE playlist_id=?1 AND position>?2 ORDER BY position LIMIT 1" };
            if let Some((other_id, other_position)) = connection.query_row(sql, params![playlist, position], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))).optional()? {
                let temporary: i64 = connection.query_row("SELECT COALESCE(MAX(position),0)+1 FROM entries WHERE playlist_id=?1", [playlist], |row| row.get(0))?;
                connection.execute("UPDATE entries SET position=?1 WHERE id=?2", params![temporary, entry])?;
                connection.execute("UPDATE entries SET position=?1 WHERE id=?2", params![position, other_id])?;
                connection.execute("UPDATE entries SET position=?1 WHERE id=?2", params![other_position, entry])?;
            }
            Ok(())
        })
    }

    pub fn move_playlist(&mut self, id: i64, direction: i64, unlocked: bool) -> Result<Change> {
        self.mutate(Some(id), Mode::Order, unlocked, |connection| {
            let (mode, kind, position): (String, String, i64) = connection.query_row("SELECT mode,kind,position FROM playlists WHERE id=?1", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            ensure!(kind == "normal", "The sorting desk has a fixed position");
            let sql = if direction < 0 { "SELECT id,position FROM playlists WHERE mode=?1 AND kind='normal' AND position<?2 ORDER BY position DESC LIMIT 1" } else { "SELECT id,position FROM playlists WHERE mode=?1 AND kind='normal' AND position>?2 ORDER BY position LIMIT 1" };
            if let Some((other, other_position)) = connection.query_row(sql, params![mode, position], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))).optional()? {
                connection.execute("UPDATE playlists SET position=?1 WHERE id=?2", params![other_position, id])?;
                connection.execute("UPDATE playlists SET position=?1 WHERE id=?2", params![position, other])?;
            }
            Ok(())
        })
    }

    pub fn rename_track(
        &mut self,
        id: i64,
        title: &str,
        playlist: i64,
        unlocked: bool,
    ) -> Result<()> {
        let title = checked_name(title)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let library = read_playlists(&transaction)?;
        let selected = library
            .iter()
            .find(|value| value.id == playlist)
            .context("Playlist no longer exists")?;
        ensure!(
            selected.can_edit(unlocked),
            "Enable Order editing before renaming tracks"
        );
        // Общий псевдоним затрагивает и другие плейлисты Порядка: они тоже должны быть разблокированы.
        ensure!(
            unlocked
                || !library.iter().any(|value| value.mode == Mode::Order
                    && value.kind == PlaylistKind::Normal
                    && value.entries.iter().any(|entry| entry.track.id == id)),
            "Track is also used by a protected Order playlist"
        );
        ensure!(
            selected.entries.iter().any(|entry| entry.track.id == id),
            "Track is not in this playlist"
        );
        transaction.execute("UPDATE tracks SET title=?1 WHERE id=?2", params![title, id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn restore(
        &mut self,
        scope: &'static str,
        expected: &Snapshot,
        replacement: &Snapshot,
    ) -> Result<()> {
        ensure!(
            matches!(scope, "chaos" | "desk"),
            "Undo is only available in Chaos and on the sorting desk"
        );
        ensure!(
            replacement.playlists.iter().all(|playlist| {
                if scope == "desk" {
                    playlist.kind == "desk" && playlist.mode == "order"
                } else {
                    playlist.kind == "normal" && playlist.mode == "chaos"
                }
            }),
            "Undo cannot change a protected playlist outside its scope"
        );
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure!(
            &snapshot(&transaction, scope)? == expected,
            "Playlist changed in another window; undo is no longer safe"
        );
        let ids: Vec<i64> = expected
            .playlists
            .iter()
            .map(|playlist| playlist.id)
            .collect();
        for id in ids {
            transaction.execute("DELETE FROM playlists WHERE id=?1", [id])?;
        }
        for playlist in &replacement.playlists {
            transaction.execute("INSERT INTO playlists(id,name,name_fold,mode,kind,position) VALUES (?1,?2,?3,?4,?5,?6)", params![playlist.id, playlist.name, playlist.name.to_lowercase(), playlist.mode, playlist.kind, playlist.position])?;
            for entry in &playlist.entries {
                transaction.execute(
                    "INSERT INTO entries(id,playlist_id,track_id,position) VALUES (?1,?2,?3,?4)",
                    params![entry.id, playlist.id, entry.track_id, entry.position],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
}

fn checked_name(name: &str) -> Result<&str> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.chars().count() <= 160 && !name.chars().any(char::is_control),
        "Name must contain 1–160 printable characters"
    );
    Ok(name)
}

fn playlist_name(name: &str) -> Result<&str> {
    let name = checked_name(name)?;
    ensure!(
        !matches!(
            name.to_lowercase().as_str(),
            "sorting desk" | "сортировочный стол"
        ),
        "This name is reserved for the sorting desk"
    );
    Ok(name)
}

fn append_track(connection: &Connection, playlist: i64, track: i64) -> Result<()> {
    connection.execute("INSERT INTO entries(playlist_id,track_id,position) SELECT ?1,?2,COALESCE(MAX(position),-1)+1 FROM entries WHERE playlist_id=?1 ON CONFLICT(playlist_id,track_id) DO NOTHING", params![playlist, track])?;
    Ok(())
}

fn read_playlists(connection: &Connection) -> Result<Vec<Playlist>> {
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
    for row in rows {
        let (id, name, mode, kind, position) = row?;
        let mode = match mode.as_str() {
            "order" => Mode::Order,
            "chaos" => Mode::Chaos,
            _ => bail!("Unknown playlist mode"),
        };
        let kind = match kind.as_str() {
            "normal" => PlaylistKind::Normal,
            "desk" => PlaylistKind::SortingDesk,
            _ => bail!("Unknown playlist type"),
        };
        let mut entries_statement = connection.prepare("SELECT e.id,e.position,t.id,t.path,t.title,t.artist,t.album,t.duration_ms,t.tags FROM entries e JOIN tracks t ON t.id=e.track_id WHERE e.playlist_id=?1 ORDER BY e.position,e.id")?;
        let entries = entries_statement
            .query_map([id], |row| {
                Ok(Entry {
                    id: row.get(0)?,
                    position: row.get(1)?,
                    track: Track {
                        id: row.get(2)?,
                        path: PathBuf::from(row.get::<_, String>(3)?),
                        title: row.get(4)?,
                        artist: row.get(5)?,
                        album: row.get(6)?,
                        duration_ms: row.get::<_, i64>(7)? as u64,
                        tags: row.get(8)?,
                    },
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        playlists.push(Playlist {
            id,
            name,
            mode,
            kind,
            position,
            entries,
        });
    }
    Ok(playlists)
}

fn snapshot(connection: &Connection, scope: &str) -> Result<Snapshot> {
    let playlists = read_playlists(connection)?
        .into_iter()
        .filter(|playlist| {
            if scope == "desk" {
                playlist.kind == PlaylistKind::SortingDesk
            } else {
                playlist.mode.key() == scope && playlist.kind == PlaylistKind::Normal
            }
        })
        .map(|playlist| SavedPlaylist {
            id: playlist.id,
            name: playlist.name,
            mode: playlist.mode.key().to_owned(),
            kind: if playlist.kind == PlaylistKind::SortingDesk {
                "desk"
            } else {
                "normal"
            }
            .to_owned(),
            position: playlist.position,
            entries: playlist
                .entries
                .into_iter()
                .map(|entry| SavedEntry {
                    id: entry.id,
                    track_id: entry.track.id,
                    position: entry.position,
                })
                .collect(),
        })
        .collect();
    Ok(Snapshot { playlists })
}
