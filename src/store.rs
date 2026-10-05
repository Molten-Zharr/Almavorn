use crate::model::{Entry, ImportedTrack, Mode, Playlist, PlaylistKind, Settings, Track};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::OnceCell,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

mod migration;

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

#[derive(Clone)]
pub struct Change {
    pub scope: &'static str,
    pub before: Snapshot,
    pub after: Snapshot,
}

pub struct Store {
    connection: OnceCell<Connection>,
    writer: Arc<Mutex<()>>,
    revision: Arc<AtomicU64>,
    pub path: PathBuf,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory).context("Cannot create data directory")?;
        let path = directory.join("almavorn.sqlite");
        let connection = migration::open(directory, &path)?;
        Ok(Self {
            connection: OnceCell::from(connection),
            writer: Arc::new(Mutex::new(())),
            revision: Arc::new(AtomicU64::new(0)),
            path,
        })
    }

    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self {
            // Opening a connection performs I/O. Delay it until the background job
            // uses this handle, so scheduling work never blocks the interface.
            connection: OnceCell::new(),
            writer: self.writer.clone(),
            revision: self.revision.clone(),
            path: self.path.clone(),
        })
    }

    fn connection(&self) -> Result<&Connection> {
        if self.connection.get().is_none() {
            self.connection
                .set(connect(&self.path)?)
                .map_err(|_| anyhow::anyhow!("Database connection was already initialized"))?;
        }
        Ok(self.connection.get().expect("connection is initialized"))
    }

    pub fn settings(&self) -> Result<Settings> {
        let json: Option<String> = self
            .connection()?
            .query_row("SELECT value FROM settings WHERE key='app'", [], |row| {
                row.get(0)
            })
            .optional()?;
        match json {
            Some(json) => {
                let value: serde_json::Value =
                    serde_json::from_str(&json).context("Saved settings are damaged")?;
                let legacy = value.get("appearance").is_none();
                let mut settings: Settings =
                    serde_json::from_value(value).context("Saved settings are damaged")?;
                if legacy {
                    settings.migrate_legacy_appearance();
                }
                settings.initialize_profiles();
                settings.validate_catalogs()?;
                Ok(settings)
            }
            None => {
                let mut settings = Settings::default();
                settings.initialize_profiles();
                Ok(settings)
            }
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        self.write(|connection| {
            connection.execute("INSERT INTO settings(key,value) VALUES ('app',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json.as_str()])?;
            Ok(())
        })
    }

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let transaction = self.connection()?.unchecked_transaction()?;
        let playlists = read_playlists(&transaction)?;
        transaction.commit()?;
        Ok(playlists)
    }

    pub fn snapshot(&self, scope: &str) -> Result<Snapshot> {
        let transaction = self.connection()?.unchecked_transaction()?;
        let snapshot = snapshot(&transaction, scope)?;
        transaction.commit()?;
        Ok(snapshot)
    }

    fn write<T>(&mut self, operation: impl Fn(&Connection) -> Result<T>) -> Result<T> {
        let writer = self.writer.clone();
        let _guard = writer
            .lock()
            .map_err(|_| anyhow::anyhow!("Database writer failed"))?;
        let result = (|| {
            let transaction =
                Transaction::new_unchecked(self.connection()?, TransactionBehavior::Immediate)?;
            let value = operation(&transaction)?;
            transaction.commit()?;
            Ok(value)
        })();
        result.map_err(|error: anyhow::Error| {
            if transaction_conflict(&error) {
                error.context(
                    "Library changed concurrently; no changes were saved. Repeat the action",
                )
            } else if error.chain().any(|cause| {
                cause
                    .to_string()
                    .contains("UNIQUE constraint failed: playlists.mode, playlists.name_fold")
            }) {
                error.context("Playlist with this name already exists")
            } else {
                error
            }
        })
    }

    fn mutate<F>(
        &mut self,
        playlist_id: Option<i64>,
        mode: Mode,
        unlocked: bool,
        operation: F,
    ) -> Result<Change>
    where
        F: Fn(&Connection) -> Result<()>,
    {
        let change = self.write(|transaction| {
            let scope = if let Some(id) = playlist_id {
                let playlist = read_playlists(transaction)?
                    .into_iter()
                    .find(|playlist| playlist.id == id)
                    .context("Playlist no longer exists")?;
                ensure!(
                    playlist.can_edit(unlocked),
                    "Enable Order editing before changing this playlist"
                );
                // The writer owns the transaction until commit; protected-state
                // checks and the corresponding changes observe the same snapshot.
                transaction
                    .execute("UPDATE playlists SET revision=revision+1 WHERE id=?1", [id])?;
                if playlist.kind == PlaylistKind::SortingDesk {
                    "desk"
                } else {
                    playlist.mode.key()
                }
            } else {
                mode.key()
            };
            let before = snapshot(transaction, scope)?;
            operation(transaction)?;
            let after = snapshot(transaction, scope)?;
            Ok(Change {
                scope,
                before,
                after,
            })
        })?;
        self.revision.fetch_add(1, Ordering::Release);
        Ok(change)
    }

    pub fn create_playlist(&mut self, name: &str, mode: Mode) -> Result<Change> {
        let name = playlist_name(name)?;
        self.mutate(None, mode, true, |connection| {
            lock_ordering(connection, mode.key())?;
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
            let mode: String =
                connection.query_row("SELECT mode FROM playlists WHERE id=?1", [id], |row| {
                    row.get(0)
                })?;
            lock_ordering(connection, &mode)?;
            connection.execute("DELETE FROM entries WHERE playlist_id=?1", [id])?;
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
            lock_ordering(connection, &mode)?;
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
        self.write(|transaction| {
            let library = read_playlists(transaction)?;
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
            transaction.execute(
                "UPDATE playlists SET revision=revision+1 WHERE id=?1",
                [playlist],
            )?;
            transaction.execute("UPDATE tracks SET title=?1 WHERE id=?2", params![title, id])?;
            Ok(())
        })?;
        self.revision.fetch_add(1, Ordering::Release);
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
            scope != "desk" || replacement.playlists.len() == 1,
            "The sorting desk cannot be deleted"
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
        self.write(|transaction| {
            lock_ordering(transaction, if scope == "desk" { "order" } else { scope })?;
            ensure!(
                &snapshot(transaction, scope)? == expected,
                "Playlist changed concurrently; undo is no longer safe"
            );
            let ids: Vec<i64> = expected
                .playlists
                .iter()
                .map(|playlist| playlist.id)
                .collect();
            for id in ids {
                transaction.execute("DELETE FROM entries WHERE playlist_id=?1", [id])?;
                transaction.execute("DELETE FROM playlists WHERE id=?1", [id])?;
            }
            for playlist in &replacement.playlists {
                transaction.execute(
                    "INSERT INTO playlists(id,name,name_fold,mode,kind,position,desk) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        playlist.id,
                        playlist.name,
                        playlist.name.to_lowercase(),
                        playlist.mode,
                        playlist.kind,
                        playlist.position,
                        (playlist.kind == "desk").then_some(true),
                    ],
                )?;
                for entry in &playlist.entries {
                    transaction.execute(
                        "INSERT INTO entries(id,playlist_id,track_id,position) VALUES (?1,?2,?3,?4)",
                        params![entry.id, playlist.id, entry.track_id, entry.position],
                    )?;
                }
            }
            Ok(())
        })?;
        self.revision.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

fn connect(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Cannot open SQLite music database")?;
    connection.busy_timeout(Duration::from_secs(3))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    Ok(connection)
}

fn lock_ordering(connection: &Connection, mode: &str) -> Result<()> {
    // Retain the ordering revision while all writes are serialized by the writer.
    ensure!(
        connection.execute(
            "UPDATE playlist_ordering SET revision=revision+1 WHERE mode=?1",
            [mode]
        )? == 1,
        "Unknown playlist mode"
    );
    Ok(())
}

fn transaction_conflict(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        matches!(
            cause.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(code, _))
                if matches!(code.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
        )
    })
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
    // Serialize new references with alias changes: an Order reference cannot appear
    // unnoticed while rename_track checks whether the shared track is protected.
    ensure!(
        connection.execute("UPDATE tracks SET revision=revision+1 WHERE id=?1", [track])? == 1,
        "Track no longer exists"
    );
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
    Ok(Snapshot::from_library(&read_playlists(connection)?, scope))
}

impl Snapshot {
    pub fn from_library(library: &[Playlist], scope: &str) -> Self {
        let playlists = library
            .iter()
            .filter(|playlist| {
                if scope == "desk" {
                    playlist.kind == PlaylistKind::SortingDesk
                } else {
                    playlist.mode.key() == scope && playlist.kind == PlaylistKind::Normal
                }
            })
            .map(|playlist| SavedPlaylist {
                id: playlist.id,
                name: playlist.name.clone(),
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
                    .iter()
                    .map(|entry| SavedEntry {
                        id: entry.id,
                        track_id: entry.track.id,
                        position: entry.position,
                    })
                    .collect(),
            })
            .collect();
        Self { playlists }
    }
}
