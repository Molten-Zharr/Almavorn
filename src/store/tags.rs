use super::Store;
use crate::{model::Track, tags::Document};
use anyhow::{Context, Result, ensure};
use rusqlite::params;

impl Store {
    pub(crate) fn write_file_tags(
        &mut self,
        desk: i64,
        track: i64,
        document: &Document,
    ) -> Result<(Track, Option<std::path::PathBuf>)> {
        let prepared = document.prepare()?;
        let imported = prepared.track.clone();
        let marker = crate::tags::recovery_path(&self.path);
        let mut replacement = None;
        let result = self.write(|connection| {
            let path: Option<String> = connection.query_row(
                "SELECT t.path FROM tracks t JOIN entries e ON e.track_id=t.id JOIN playlists p ON p.id=e.playlist_id WHERE t.id=?1 AND p.id=?2 AND p.kind='desk'",
                params![track, desk], |row| row.get(0),
            ).optional()?;
            ensure!(path.as_deref() == document.path.to_str(),
                "Композиция больше не находится на сортировочном столе. Откройте редактор заново.");
            prepared.record_recovery(&marker, track)?;
            connection.execute(
                "UPDATE tracks SET title=?1,artist=?2,album=?3,duration_ms=?4,tags=?5,revision=revision+1 WHERE id=?6",
                params![imported.title, imported.artist, imported.album, imported.duration_ms.min(i64::MAX as u64) as i64, imported.tags, track],
            )?;
            replacement = Some(prepared.replace()?);
            Ok(())
        });
        if let Err(error) = result {
            if let Some(replacement) = replacement {
                replacement.rollback()?;
            }
            crate::tags::finish_recovery(&marker)?;
            return Err(error).context("Не удалось сохранить теги и обновить библиотеку.");
        }
        let preserved = replacement
            .context("Запись файла не выполнена.")?
            .accept()?;
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        crate::tags::finish_recovery(&marker).context(
            "Теги записаны, но не удалось удалить запись восстановления. Перезапустите приложение.",
        )?;
        Ok((
            Track {
                id: track,
                path: imported.path,
                title: imported.title,
                artist: imported.artist,
                album: imported.album,
                duration_ms: imported.duration_ms,
                tags: imported.tags,
            },
            preserved,
        ))
    }
}

use rusqlite::OptionalExtension;
