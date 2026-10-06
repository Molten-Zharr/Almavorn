use super::connect;
use crate::model::Settings;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::Deserialize;
use std::{path::Path, process::Command, time::Duration};

const SCHEMA: &str = "
CREATE TABLE tracks (
    id INTEGER PRIMARY KEY AUTOINCREMENT, path TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL, artist TEXT NOT NULL, album TEXT NOT NULL,
    duration_ms INTEGER NOT NULL CHECK(duration_ms>=0), tags TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE playlists (
    id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, name_fold TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('order','chaos')),
    kind TEXT NOT NULL CHECK(kind IN ('normal','desk')), position INTEGER NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0, desk INTEGER UNIQUE, UNIQUE(mode,name_fold),
    CHECK ((kind='desk' AND mode='order' AND position=-1 AND desk IS 1)
        OR (kind='normal' AND position>=0 AND desk IS NULL))
);
CREATE TABLE entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id INTEGER NOT NULL REFERENCES tracks(id),
    position INTEGER NOT NULL CHECK(position>=0),
    UNIQUE(playlist_id,position), UNIQUE(playlist_id,track_id)
);
CREATE INDEX entries_track ON entries(track_id);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE almavorn_metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE playlist_ordering (mode TEXT PRIMARY KEY CHECK(mode IN ('order','chaos')), revision INTEGER NOT NULL);
INSERT INTO playlist_ordering VALUES ('order',0),('chaos',0);
";

const FOLDER_SCHEMA: &str = "CREATE TABLE playlist_folders (
    playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    path TEXT NOT NULL, PRIMARY KEY(playlist_id,path)
);";

#[derive(Deserialize)]
struct Library {
    tracks: Option<Vec<Track>>,
    playlists: Option<Vec<Playlist>>,
    entries: Option<Vec<Entry>>,
    settings: Option<Vec<Setting>>,
    metadata: Option<Vec<Setting>>,
    ordering: Option<Vec<Ordering>>,
    sequences: Option<Vec<Sequence>>,
}
#[derive(Deserialize)]
struct Track {
    id: i64,
    path: String,
    title: String,
    artist: String,
    album: String,
    duration_ms: i64,
    tags: String,
    revision: i64,
}
#[derive(Deserialize)]
struct Playlist {
    id: i64,
    name: String,
    name_fold: String,
    mode: String,
    kind: String,
    position: i64,
    revision: i64,
}
#[derive(Deserialize)]
struct Entry {
    id: i64,
    playlist_id: i64,
    track_id: i64,
    position: i64,
}
#[derive(Deserialize)]
struct Setting {
    key: String,
    value: String,
}
#[derive(Deserialize)]
struct Ordering {
    mode: String,
    revision: i64,
}
#[derive(Deserialize)]
struct Sequence {
    sequence_name: String,
    start_value: i64,
    last_value: Option<i64>,
}

#[derive(Deserialize)]
struct Row {
    section: String,
    a: String,
    b: Option<String>,
    c: Option<String>,
    d: Option<String>,
    e: Option<String>,
    f: Option<String>,
    g: Option<String>,
    h: Option<String>,
}
impl Row {
    fn value(&self, column: usize) -> Result<&str> {
        [
            Some(self.a.as_str()),
            self.b.as_deref(),
            self.c.as_deref(),
            self.d.as_deref(),
            self.e.as_deref(),
            self.f.as_deref(),
            self.g.as_deref(),
            self.h.as_deref(),
        ]
        .get(column)
        .copied()
        .flatten()
        .context("Missing source value; original database was preserved")
    }
    fn integer(&self, column: usize) -> Result<i64> {
        self.value(column)?
            .parse()
            .context("Invalid source integer; original database was preserved")
    }
}

pub(super) fn open(directory: &Path, path: &Path) -> Result<Connection> {
    if path.try_exists()? {
        let mut connection = connect(path)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if matches!(version, 2 | 3) {
            if version == 2 {
                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                upgrade_folders(&transaction)?;
                transaction.commit()?;
            } else {
                validate_schema(&connection, version)?;
            }
            configure_wal(&connection)?;
            return Ok(connection);
        }
        ensure!(
            version == 0 && empty(&connection)?,
            "Unrecognized SQLite library; existing data was preserved / Неизвестная версия базы SQLite; исходные данные сохранены"
        );
    }

    // Read the source before creating the destination. DuckDB always takes
    // precedence over its older SQLite source; a failed import never falls back.
    let duckdb = directory.join("almavorn.duckdb");
    let legacy = directory.join("almavorn.sqlite3");
    let imported = if duckdb.try_exists()? {
        Some(read_duckdb(&duckdb)?)
    } else {
        None
    };
    let legacy = if imported.is_none() && legacy.try_exists()? {
        let source = Connection::open_with_flags(
            &legacy,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        source.busy_timeout(Duration::from_secs(3))?;
        // No source DML is executed. The empty writer transaction excludes late
        // commits during handoff and is rolled back when the connection closes.
        source.execute_batch("BEGIN IMMEDIATE").context("Close the old player before migrating SQLite / Перед переносом SQLite закройте старый плеер; исходная база сохранена")?;
        Some(source)
    } else {
        None
    };

    let mut connection = Connection::open(path).context("Cannot create SQLite music database")?;
    connection.busy_timeout(Duration::from_secs(3))?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if matches!(version, 2 | 3) {
        upgrade_folders(&transaction)?;
        transaction.commit()?;
        configure_wal(&connection)?;
        return Ok(connection);
    }
    ensure!(
        version == 0 && empty(&transaction)?,
        "Unrecognized SQLite library; existing data was preserved"
    );
    transaction.execute_batch(SCHEMA)?;
    transaction.execute_batch(FOLDER_SCHEMA)?;
    if let Some(library) = imported {
        import_duckdb(&transaction, library)?;
    } else if let Some(source) = &legacy {
        import_sqlite(source, &transaction)?;
    } else {
        transaction.execute("INSERT INTO playlists(name,name_fold,mode,kind,position,desk) VALUES ('Sorting desk','sorting desk','order','desk',-1,1)", [])?;
    }
    infer_legacy_folders(&transaction)?;
    let desks: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM playlists WHERE kind='desk'",
        [],
        |row| row.get(0),
    )?;
    ensure!(desks == 1, "Library must contain exactly one sorting desk");
    let broken = transaction
        .prepare("PRAGMA foreign_key_check")?
        .exists([])?;
    ensure!(
        !broken,
        "Library contains broken references; original databases were preserved"
    );
    transaction.execute("INSERT INTO almavorn_metadata(key,value) VALUES ('schema_version','3') ON CONFLICT(key) DO UPDATE SET value=excluded.value", [])?;
    transaction.pragma_update(None, "user_version", 3)?;
    transaction.commit()?;
    configure_wal(&connection)?;
    Ok(connection)
}

fn empty(connection: &Connection) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get::<_, i64>(0),
    )? == 0)
}
fn upgrade_folders(connection: &Connection) -> Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    validate_schema(connection, version)?;
    if version == 2 {
        connection.execute_batch(FOLDER_SCHEMA)?;
        infer_legacy_folders(connection)?;
        connection.execute(
            "UPDATE almavorn_metadata SET value='3' WHERE key='schema_version'",
            [],
        )?;
        connection.pragma_update(None, "user_version", 3)?;
    }
    Ok(())
}

fn infer_legacy_folders(connection: &Connection) -> Result<()> {
    let mut statement = connection
        .prepare("SELECT e.playlist_id,t.path FROM entries e JOIN tracks t ON t.id=e.track_id")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, path) in rows {
        if let Some(folder) = Path::new(&path).parent().filter(|path| path.is_absolute()) {
            connection.execute("INSERT INTO playlist_folders(playlist_id,path) VALUES (?1,?2) ON CONFLICT DO NOTHING", params![id, folder.to_str()])?;
        }
    }
    Ok(())
}

fn validate_schema(connection: &Connection, version: i64) -> Result<()> {
    let tables: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name IN ('tracks','playlists','entries','settings','almavorn_metadata','playlist_ordering')", [], |row| row.get(0))?;
    ensure!(
        tables == 6,
        "Unrecognized SQLite schema; existing data was preserved"
    );
    let marker: Option<String> = connection
        .query_row(
            "SELECT value FROM almavorn_metadata WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    ensure!(
        marker.as_deref() == Some(if version == 2 { "2" } else { "3" }),
        "Unrecognized SQLite schema marker; existing data was preserved"
    );
    connection.prepare(
        "SELECT id,path,title,artist,album,duration_ms,tags,revision FROM tracks LIMIT 0",
    )?;
    connection.prepare(
        "SELECT id,name,name_fold,mode,kind,position,revision,desk FROM playlists LIMIT 0",
    )?;
    connection.prepare("SELECT id,playlist_id,track_id,position FROM entries LIMIT 0")?;
    connection.prepare("SELECT key,value FROM settings LIMIT 0")?;
    connection.prepare("SELECT mode,revision FROM playlist_ordering LIMIT 0")?;
    if version == 3 {
        connection.prepare("SELECT playlist_id,path FROM playlist_folders LIMIT 0")?;
    }
    Ok(())
}
fn configure_wal(connection: &Connection) -> Result<()> {
    let journal: String =
        connection.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    ensure!(
        journal.eq_ignore_ascii_case("wal"),
        "SQLite WAL could not be enabled"
    );
    Ok(())
}

fn read_duckdb(path: &Path) -> Result<Library> {
    // Scalar columns let the CLI serialize JSON without its SQL JSON extension.
    // The single SELECT observes a consistent snapshot of every source table.
    let query = "SET enable_external_access=false;
    SELECT 'tracks' AS section,id::VARCHAR AS a,path AS b,title AS c,artist AS d,
        album AS e,duration_ms::VARCHAR AS f,tags AS g,revision::VARCHAR AS h FROM tracks
    UNION ALL SELECT 'playlists',id::VARCHAR,name,name_fold,mode,kind,position::VARCHAR,revision::VARCHAR,NULL FROM playlists
    UNION ALL SELECT 'entries',id::VARCHAR,playlist_id::VARCHAR,track_id::VARCHAR,position::VARCHAR,NULL,NULL,NULL,NULL FROM entries
    UNION ALL SELECT 'settings',key,value,NULL,NULL,NULL,NULL,NULL,NULL FROM settings
    UNION ALL SELECT 'metadata',key,value,NULL,NULL,NULL,NULL,NULL,NULL FROM almavorn_metadata
    UNION ALL SELECT 'ordering',mode,revision::VARCHAR,NULL,NULL,NULL,NULL,NULL,NULL FROM playlist_ordering
    UNION ALL SELECT 'sequences',sequence_name,start_value::VARCHAR,last_value::VARCHAR,NULL,NULL,NULL,NULL,NULL FROM duckdb_sequences() WHERE schema_name='main'
    ORDER BY section,a;";
    let output = Command::new("duckdb")
        .args(["-init", if cfg!(windows) { "NUL" } else { "/dev/null" }])
        .args(["-readonly", "-json", "-batch", "-no-stdin"])
        .arg(path.canonicalize()?).arg(query).output()
        .context("To migrate the existing library, install DuckDB CLI and close the old player / Для переноса существующей библиотеки установите DuckDB CLI и закройте старый плеер; исходная база сохранена")?;
    ensure!(
        output.status.success(),
        "DuckDB import failed; source preserved / Перенос DuckDB не выполнен; исходная база сохранена: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let rows: Vec<Row> = serde_json::from_slice(&output.stdout).context("Invalid DuckDB snapshot; source preserved / Некорректный снимок DuckDB; исходная база сохранена")?;
    let mut tracks = Vec::new();
    let mut playlists = Vec::new();
    let mut entries = Vec::new();
    let mut settings = Vec::new();
    let mut metadata = Vec::new();
    let mut ordering = Vec::new();
    let mut sequences = Vec::new();
    for row in rows {
        match row.section.as_str() {
            "tracks" => tracks.push(Track {
                id: row.integer(0)?,
                path: row.value(1)?.into(),
                title: row.value(2)?.into(),
                artist: row.value(3)?.into(),
                album: row.value(4)?.into(),
                duration_ms: row.integer(5)?,
                tags: row.value(6)?.into(),
                revision: row.integer(7)?,
            }),
            "playlists" => playlists.push(Playlist {
                id: row.integer(0)?,
                name: row.value(1)?.into(),
                name_fold: row.value(2)?.into(),
                mode: row.value(3)?.into(),
                kind: row.value(4)?.into(),
                position: row.integer(5)?,
                revision: row.integer(6)?,
            }),
            "entries" => entries.push(Entry {
                id: row.integer(0)?,
                playlist_id: row.integer(1)?,
                track_id: row.integer(2)?,
                position: row.integer(3)?,
            }),
            "settings" => settings.push(Setting {
                key: row.value(0)?.into(),
                value: row.value(1)?.into(),
            }),
            "metadata" => metadata.push(Setting {
                key: row.value(0)?.into(),
                value: row.value(1)?.into(),
            }),
            "ordering" => ordering.push(Ordering {
                mode: row.value(0)?.into(),
                revision: row.integer(1)?,
            }),
            "sequences" => sequences.push(Sequence {
                sequence_name: row.value(0)?.into(),
                start_value: row.integer(1)?,
                last_value: row.c.as_deref().map(str::parse).transpose()?,
            }),
            _ => anyhow::bail!("Unknown source section; original database was preserved"),
        }
    }
    ensure!(
        metadata
            .iter()
            .filter(|row| row.key == "schema_version" && row.value == "1")
            .count()
            == 1,
        "Unsupported DuckDB schema; source preserved"
    );
    ensure!(
        ordering.len() == 2
            && ["order", "chaos"].iter().all(|mode| ordering
                .iter()
                .filter(|row| row.mode == *mode)
                .count()
                == 1),
        "Invalid source ordering; original database was preserved"
    );
    ensure!(
        sequences.len() == 3
            && ["tracks_ids", "playlists_ids", "entries_ids"]
                .iter()
                .all(|name| sequences
                    .iter()
                    .filter(|row| row.sequence_name == *name)
                    .count()
                    == 1),
        "Invalid source sequences; original database was preserved"
    );
    Ok(Library {
        tracks: Some(tracks),
        playlists: Some(playlists),
        entries: Some(entries),
        settings: Some(settings),
        metadata: Some(metadata),
        ordering: Some(ordering),
        sequences: Some(sequences),
    })
}

fn import_duckdb(connection: &Connection, library: Library) -> Result<()> {
    for t in library.tracks.unwrap_or_default() {
        connection.execute("INSERT INTO tracks(id,path,title,artist,album,duration_ms,tags,revision) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![t.id,t.path,t.title,t.artist,t.album,t.duration_ms,t.tags,t.revision])?;
    }
    for p in library.playlists.unwrap_or_default() {
        connection.execute("INSERT INTO playlists(id,name,name_fold,mode,kind,position,revision,desk) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", params![p.id,p.name,p.name_fold,p.mode,p.kind,p.position,p.revision,(p.kind=="desk").then_some(1)])?;
    }
    for e in library.entries.unwrap_or_default() {
        connection.execute(
            "INSERT INTO entries(id,playlist_id,track_id,position) VALUES (?1,?2,?3,?4)",
            params![e.id, e.playlist_id, e.track_id, e.position],
        )?;
    }
    for s in library.settings.unwrap_or_default() {
        setting(connection, s)?;
    }
    for m in library.metadata.unwrap_or_default() {
        connection.execute(
            "INSERT INTO almavorn_metadata VALUES (?1,?2)",
            params![m.key, m.value],
        )?;
    }
    for o in library.ordering.unwrap_or_default() {
        connection.execute(
            "UPDATE playlist_ordering SET revision=?1 WHERE mode=?2",
            params![o.revision, o.mode],
        )?;
    }
    for sequence in library.sequences.unwrap_or_default() {
        let table = match sequence.sequence_name.as_str() {
            "tracks_ids" => "tracks",
            "playlists_ids" => "playlists",
            "entries_ids" => "entries",
            _ => anyhow::bail!("Unknown source sequence; original database was preserved"),
        };
        let high_water = sequence
            .last_value
            .unwrap_or(0)
            .max(sequence.start_value.saturating_sub(1));
        preserve_sequence(connection, table, high_water)?;
    }
    Ok(())
}

fn import_sqlite(source: &Connection, destination: &Connection) -> Result<()> {
    let snapshot = source;
    let version: i64 = snapshot.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == 1,
        "Unsupported old SQLite schema; original database was preserved"
    );
    let mut statement = snapshot
        .prepare("SELECT id,path,title,artist,album,duration_ms,tags FROM tracks ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(r) = rows.next()? {
        destination.execute("INSERT INTO tracks(id,path,title,artist,album,duration_ms,tags) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?,r.get::<_,String>(6)?])?;
    }
    let mut statement = snapshot
        .prepare("SELECT id,name,name_fold,mode,kind,position FROM playlists ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(r) = rows.next()? {
        let kind: String = r.get(4)?;
        destination.execute("INSERT INTO playlists(id,name,name_fold,mode,kind,position,desk) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,kind,r.get::<_,i64>(5)?,(kind=="desk").then_some(1)])?;
    }
    let mut statement =
        snapshot.prepare("SELECT id,playlist_id,track_id,position FROM entries ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(r) = rows.next()? {
        destination.execute(
            "INSERT INTO entries(id,playlist_id,track_id,position) VALUES (?1,?2,?3,?4)",
            params![
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?
            ],
        )?;
    }
    let mut statement = snapshot.prepare("SELECT key,value FROM settings ORDER BY key")?;
    let mut rows = statement.query([])?;
    while let Some(r) = rows.next()? {
        setting(
            destination,
            Setting {
                key: r.get(0)?,
                value: r.get(1)?,
            },
        )?;
    }
    for table in ["tracks", "playlists", "entries"] {
        let high_water: Option<i64> = snapshot
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name=?1",
                [table],
                |row| row.get(0),
            )
            .optional()?;
        preserve_sequence(destination, table, high_water.unwrap_or(0))?;
    }
    Ok(())
}
fn setting(connection: &Connection, setting: Setting) -> Result<()> {
    if setting.key == "app" {
        serde_json::from_str::<Settings>(&setting.value)
            .context("Saved source settings are damaged; original data was preserved")?;
    }
    connection.execute(
        "INSERT INTO settings VALUES (?1,?2)",
        params![setting.key, setting.value],
    )?;
    Ok(())
}
fn preserve_sequence(connection: &Connection, table: &str, high_water: i64) -> Result<()> {
    ensure!(
        high_water >= 0,
        "Invalid source sequence; original data was preserved"
    );
    connection.execute("INSERT INTO sqlite_sequence(name,seq) SELECT ?1,?2 WHERE NOT EXISTS(SELECT 1 FROM sqlite_sequence WHERE name=?1)", params![table,high_water])?;
    connection.execute(
        "UPDATE sqlite_sequence SET seq=MAX(seq,?1) WHERE name=?2",
        params![high_water, table],
    )?;
    Ok(())
}
