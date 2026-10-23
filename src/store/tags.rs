use super::Store;
use crate::{model::Track, tags::Document};
use anyhow::{Context, Result, ensure};
use rusqlite::params;
use std::path::{Path, PathBuf};

pub(crate) struct FileEditOutcome {
    pub track: Track,
    pub old_path: PathBuf,
    pub destination: Option<i64>,
    pub warning: Option<String>,
}

impl Store {
    pub(crate) fn edit_audio_file(
        &mut self,
        desk: i64,
        track: i64,
        document: &Document,
        destination: &Path,
        playlist: Option<i64>,
        allow_order: bool,
    ) -> Result<FileEditOutcome> {
        let destination = destination
            .parent()
            .context("Выберите папку назначения.")?
            .canonicalize()?
            .join(destination.file_name().context("Укажите имя файла.")?);
        ensure!(
            destination.extension() == document.path.extension(),
            "Расширение аудиофайла менять нельзя."
        );
        let marker = crate::tags::recovery_path(&self.path);
        ensure!(
            !marker.exists(),
            "Предыдущая запись файла требует восстановления. Перезапустите приложение."
        );
        let prepared = if document.changed() || destination != document.path {
            Some(document.prepare_at(&destination)?)
        } else {
            None
        };
        ensure!(
            prepared.is_some() || playlist.is_some(),
            "Нет изменений для записи."
        );
        let mut replacement = None;
        let mut recorded = false;
        let operation = prepared.as_ref().map(|p| p.operation_id());
        let result = self.write(|connection| {
            if prepared.is_none() { document.verify_source()?; }
            let path: Option<String> = connection.query_row(
                "SELECT t.path FROM tracks t JOIN entries e ON e.track_id=t.id JOIN playlists p ON p.id=e.playlist_id WHERE t.id=?1 AND p.id=?2 AND p.kind='desk'",
                params![track, desk], |row| row.get(0),
            ).optional()?;
            ensure!(path.as_deref() == document.path.to_str(),
                "Композиция больше не находится на сортировочном столе. Откройте редактор заново.");
            let target_path = destination.to_str().context("Имя файла должно быть в UTF-8.")?;
            let occupied: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM tracks WHERE path=?1 AND id!=?2)", params![target_path, track], |row| row.get(0))?;
            ensure!(!occupied, "Этот путь уже занят другой композицией в библиотеке.");
            if let Some(playlist) = playlist {
                let (mode, kind) = super::queries::playlist_access(connection, playlist)?;
                ensure!(kind == crate::model::PlaylistKind::Normal, "Выберите обычный плейлист.");
                ensure!(mode != crate::model::Mode::Order || allow_order, crate::errors::AppError::OrderProtected);
                // Keep the existing entry ID when the track is already in the target.
                super::append_track(connection, playlist, track)?;
                let first: i64 = connection.query_row("SELECT id FROM entries WHERE playlist_id=?1 AND track_id=?2",
                    params![playlist, track], |row| row.get(0))?;
                let mut statement = connection.prepare("SELECT id FROM entries WHERE playlist_id=?1 ORDER BY position,id")?;
                let mut ids = statement.query_map([playlist], |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                ids.retain(|id| *id != first);
                ids.insert(0, first);
                let maximum: i64 = connection.query_row("SELECT COALESCE(MAX(position),0) FROM entries WHERE playlist_id=?1", [playlist], |row| row.get(0))?;
                let offset = maximum.checked_add(ids.len() as i64).and_then(|n| n.checked_add(1)).context("Слишком много позиций в плейлисте.")?;
                // Unique positions must remain unique during both passes.
                for (position, id) in ids.iter().enumerate() {
                    connection.execute("UPDATE entries SET position=?1 WHERE id=?2", params![offset.checked_add(position as i64).context("Слишком много позиций.")?, id])?;
                }
                for (position, id) in ids.iter().enumerate() {
                    connection.execute("UPDATE entries SET position=?1 WHERE id=?2", params![position as i64, id])?;
                }
                connection.execute("DELETE FROM entries WHERE playlist_id=?1 AND track_id=?2", params![desk, track])?;
                connection.execute("UPDATE playlists SET revision=revision+1 WHERE id IN (?1,?2)", params![desk, playlist])?;
                connection.execute("INSERT INTO playlist_folders(playlist_id,path) VALUES (?1,?2) ON CONFLICT DO NOTHING",
                    params![playlist, destination.parent().and_then(Path::to_str)])?;
            }
            if let Some(prepared) = prepared {
                let imported = &prepared.track;
                connection.execute("UPDATE tracks SET path=?1,title=?2,artist=?3,album=?4,duration_ms=?5,tags=?6,revision=revision+1 WHERE id=?7",
                    params![target_path, imported.title, imported.artist, imported.album, imported.duration_ms.min(i64::MAX as u64) as i64, imported.tags, track])?;
                if destination != document.path {
                    // The effect settings follow the file in the same transaction.
                    if let Some(json) = connection.query_row("SELECT value FROM settings WHERE key='app'", [], |row| row.get::<_, String>(0)).optional()? {
                        let mut value: serde_json::Value = serde_json::from_str(&json)?;
                        if let Some(tuners) = value.get_mut("track_tuners").and_then(serde_json::Value::as_object_mut)
                            && let Some(tuner) = document.path.to_str().and_then(|old| tuners.remove(old)) {
                            tuners.insert(target_path.into(), tuner);
                            connection.execute("UPDATE settings SET value=?1 WHERE key='app'", [serde_json::to_string(&value)?])?;
                        }
                    }
                    connection.execute("INSERT INTO playlist_folders(playlist_id,path) SELECT playlist_id,?1 FROM entries WHERE track_id=?2 ON CONFLICT DO NOTHING",
                        params![destination.parent().and_then(Path::to_str), track])?;
                }
                prepared.record_recovery(&marker, track)?;
                recorded = true;
                connection.execute("INSERT INTO almavorn_metadata(key,value) VALUES ('file_write_commit',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [prepared.operation_id()])?;
                replacement = Some(prepared.replace()?);
            }
            connection.query_row("SELECT path,title,artist,album,duration_ms,tags FROM tracks WHERE id=?1", [track], |row| Ok(Track {
                id: track, path: PathBuf::from(row.get::<_, String>(0)?), title: row.get(1)?, artist: row.get(2)?,
                album: row.get(3)?, duration_ms: row.get::<_, i64>(4)? as u64, tags: row.get(5)?,
            })).map_err(Into::into)
        });
        let track = match result {
            Ok(track) => track,
            Err(error) => {
                if !recorded {
                    return Err(error).context("Не удалось сохранить файл и обновить библиотеку.");
                }
                // A COMMIT I/O error does not establish whether the commit happened.
                // Release any still-open transaction, then observe durable state
                // through a new connection before choosing a filesystem rollback.
                if !self.connection()?.is_autocommit() {
                    self.connection()?.execute_batch("ROLLBACK")
                        .context("Не удалось завершить транзакцию. Копии файла сохранены для восстановления.")?;
                }
                let observer = super::connect(&self.path)
                    .context("Не удалось проверить результат записи базы. Копии файла сохранены для восстановления.")?;
                if crate::tags::file_write_committed(
                    &observer,
                    operation
                        .as_deref()
                        .context("Не найден номер операции записи.")?,
                )? {
                    observer.query_row(
                        "SELECT path,title,artist,album,duration_ms,tags FROM tracks WHERE id=?1",
                        [track],
                        |row| {
                            Ok(Track {
                                id: track,
                                path: PathBuf::from(row.get::<_, String>(0)?),
                                title: row.get(1)?,
                                artist: row.get(2)?,
                                album: row.get(3)?,
                                duration_ms: row.get::<_, i64>(4)? as u64,
                                tags: row.get(5)?,
                            })
                        },
                    )?
                } else {
                    if let Some(replacement) = replacement.take() {
                        replacement.rollback()?;
                    }
                    crate::tags::recover_file_write(&observer, &self.path)?;
                    return Err(error).context("Не удалось сохранить файл и обновить библиотеку.");
                }
            }
        };
        self.revision
            .fetch_add(1, std::sync::atomic::Ordering::Release);
        // A cleanup failure happens after commit. Report the new path rather than
        // restoring a stale editor that could write the old file a second time.
        let mut warning = None;
        if let Some(replacement) = replacement {
            match replacement.accept() {
                Ok(preserved) => {
                    if let Some(path) = preserved {
                        warning = Some(format!(
                            "Изменения сохранены. Измененный другой программой исходник оставлен: {}",
                            path.display()
                        ));
                    }
                    if let Err(error) = crate::tags::finish_recovery(&marker) {
                        let message =
                            format!("Файл сохранен. Не завершена очистка копии: {error:#}");
                        warning = Some(warning.map_or_else(
                            || message.clone(),
                            |previous| format!("{previous}\n{message}"),
                        ));
                    }
                }
                Err(error) => {
                    warning = Some(format!(
                        "Файл сохранен по новому пути. Исходная копия оставлена для восстановления: {error:#}"
                    ))
                }
            }
        }
        Ok(FileEditOutcome {
            track,
            old_path: document.path.clone(),
            destination: playlist,
            warning,
        })
    }
}

use rusqlite::OptionalExtension;
