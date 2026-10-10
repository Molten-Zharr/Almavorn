use super::Store;
use crate::statistics::{ListeningRecord, ListeningStatistics, StatisticsRow};
use anyhow::{Result, ensure};
use rusqlite::params;
use std::collections::BTreeMap;

pub(super) const SCHEMA: &str = "
CREATE TABLE listening_sessions (
    session_id TEXT PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES tracks(id),
    duration_ms INTEGER NOT NULL CHECK(duration_ms>=0),
    started_at_ms INTEGER NOT NULL, updated_at_ms INTEGER NOT NULL,
    listened_ms INTEGER NOT NULL CHECK(listened_ms>=0),
    qualified INTEGER NOT NULL CHECK(qualified IN (0,1)), ended_at_ms INTEGER
);
CREATE INDEX listening_track ON listening_sessions(track_id);
CREATE INDEX listening_recent ON listening_sessions(started_at_ms DESC);
";

impl Store {
    /// Absolute, idempotent checkpoints allow retries without counting twice.
    /// Listening does not change the library revision or its undo snapshots.
    pub fn record_listening(&mut self, records: &[ListeningRecord]) -> Result<()> {
        self.write(|connection| {
            let mut statement = connection.prepare("INSERT INTO listening_sessions
                (session_id,track_id,duration_ms,started_at_ms,updated_at_ms,listened_ms,qualified,ended_at_ms)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
                ON CONFLICT(session_id) DO UPDATE SET
                    listened_ms=MAX(listening_sessions.listened_ms,excluded.listened_ms),
                    qualified=MAX(listening_sessions.qualified,excluded.qualified),
                    updated_at_ms=MAX(listening_sessions.updated_at_ms,excluded.updated_at_ms),
                    ended_at_ms=COALESCE(excluded.ended_at_ms,listening_sessions.ended_at_ms)
                WHERE listening_sessions.track_id=excluded.track_id
                    AND listening_sessions.started_at_ms=excluded.started_at_ms")?;
            for record in records.iter().filter(|record| record.listened_ms > 0) {
                let changed = statement.execute(params![record.session_id, record.track_id,
                    i64::try_from(record.duration_ms)?, record.started_at_ms, record.updated_at_ms,
                    i64::try_from(record.listened_ms)?, record.qualified, record.ended_at_ms])?;
                ensure!(changed == 1, "Conflicting listening session identity");
            }
            Ok(())
        })
    }

    pub fn listening_statistics(&self) -> Result<ListeningStatistics> {
        let transaction = self.connection()?.unchecked_transaction()?;
        let tracks = transaction
            .prepare(
                "SELECT t.title,t.artist,t.album,SUM(s.qualified),
            COUNT(*),SUM(s.listened_ms),MAX(s.updated_at_ms),
            strftime('%d.%m.%Y %H:%M',MAX(s.updated_at_ms)/1000.0,'unixepoch','localtime')
            FROM listening_sessions s JOIN tracks t ON t.id=s.track_id
            GROUP BY t.id",
            )?
            .query_map([], |row| {
                Ok(StatisticsRow {
                    title: row.get(0)?,
                    artist: row.get(1)?,
                    album: row.get(2)?,
                    plays: row.get::<_, i64>(3)? as u64,
                    starts: row.get::<_, i64>(4)? as u64,
                    listened_ms: row.get::<_, i64>(5)? as u64,
                    tracks: 1,
                    last_at_ms: row.get(6)?,
                    date: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut result = ListeningStatistics {
            tracks,
            ..Default::default()
        };
        let mut artists = BTreeMap::<String, StatisticsRow>::new();
        let mut albums = BTreeMap::<(String, String), StatisticsRow>::new();
        for track in &result.tracks {
            result.summary.listened_ms += track.listened_ms;
            result.summary.plays += track.plays;
            result.summary.starts += track.starts;
            let artist = track.artist.trim();
            let album = track.album.trim();
            // Rust lowercasing also groups Cyrillic case variants correctly.
            let artist_row =
                artists
                    .entry(artist.to_lowercase())
                    .or_insert_with(|| StatisticsRow {
                        title: artist.into(),
                        artist: artist.into(),
                        ..Default::default()
                    });
            aggregate(artist_row, track);
            let album_row = albums
                .entry((artist.to_lowercase(), album.to_lowercase()))
                .or_insert_with(|| StatisticsRow {
                    title: album.into(),
                    artist: artist.into(),
                    ..Default::default()
                });
            aggregate(album_row, track);
        }
        result.artists = artists.into_values().collect();
        result.albums = albums.into_values().collect();
        result.summary.tracks = result.tracks.len();
        result.summary.artists = result.artists.len();
        result.summary.albums = result.albums.len();
        result.recent = transaction
            .prepare(
                "SELECT t.title,t.artist,t.album,s.qualified,
            s.listened_ms,s.updated_at_ms,
            strftime('%d.%m.%Y %H:%M',s.started_at_ms/1000.0,'unixepoch','localtime')
            FROM listening_sessions s JOIN tracks t ON t.id=s.track_id
            ORDER BY s.started_at_ms DESC,s.session_id DESC LIMIT 100",
            )?
            .query_map([], |row| {
                Ok(StatisticsRow {
                    title: row.get(0)?,
                    artist: row.get(1)?,
                    album: row.get(2)?,
                    plays: row.get::<_, i64>(3)? as u64,
                    starts: 1,
                    listened_ms: row.get::<_, i64>(4)? as u64,
                    last_at_ms: row.get(5)?,
                    date: row.get(6)?,
                    tracks: 1,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        transaction.commit()?;
        Ok(result)
    }
}

fn aggregate(row: &mut StatisticsRow, track: &StatisticsRow) {
    row.plays += track.plays;
    row.starts += track.starts;
    row.listened_ms += track.listened_ms;
    row.tracks += 1;
    if track.last_at_ms > row.last_at_ms {
        row.last_at_ms = track.last_at_ms;
        row.date.clone_from(&track.date);
    }
}
