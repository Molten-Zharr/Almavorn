use super::Store;
use crate::lyrics::Lyric;
use anyhow::{Context, Result, ensure};
use rusqlite::{OptionalExtension, params};

pub(super) const SCHEMA: &str = "CREATE TABLE saved_lyrics (
    track_id INTEGER PRIMARY KEY,
    value TEXT
);";

impl Store {
    // Streamed tracks have stable IDs too, but may have no row in the local
    // tracks table. Playlist undo and file moves must not discard saved lyrics.
    pub(crate) fn saved_lyrics(&mut self, track: i64) -> Result<Option<Lyric>> {
        let read = |store: &Self| -> Result<Option<Option<String>>> {
            Ok(store
                .connection()?
                .query_row(
                    "SELECT value FROM saved_lyrics WHERE track_id=?1",
                    [track],
                    |row| row.get(0),
                )
                .optional()?)
        };
        let mut value = read(self)?;
        if value.is_none() {
            let data = self.path.parent().context("Не найдена папка библиотеки.")?;
            if let Some(lyric) = crate::lyrics::cached(data, track)? {
                let json = serde_json::to_string(&lyric)?;
                ensure!(
                    json.len() <= 1024 * 1024,
                    "Сохраненный текст песни слишком большой."
                );
                self.write(|connection| {
                    connection.execute("INSERT INTO saved_lyrics(track_id,value) VALUES (?1,?2) ON CONFLICT(track_id) DO NOTHING", params![track, json])?;
                    Ok(())
                })?;
                value = read(self)?;
            }
        }
        value
            .flatten()
            .map(|json| {
                ensure!(
                    json.len() <= 1024 * 1024,
                    "Сохраненный текст песни слишком большой."
                );
                serde_json::from_str(&json).context("Не удалось прочитать сохраненный текст песни.")
            })
            .transpose()
    }

    pub(crate) fn save_lyrics(&mut self, track: i64, lyric: Option<&Lyric>) -> Result<()> {
        let json = lyric.map(serde_json::to_string).transpose()?;
        ensure!(
            json.as_ref().is_none_or(|json| json.len() <= 1024 * 1024),
            "Текст песни слишком большой для сохранения."
        );
        self.write(|connection| {
            // NULL also prevents a removed legacy JSON copy from being imported again.
            connection.execute("INSERT INTO saved_lyrics(track_id,value) VALUES (?1,?2) ON CONFLICT(track_id) DO UPDATE SET value=excluded.value", params![track, json])?;
            Ok(())
        })
    }
}
