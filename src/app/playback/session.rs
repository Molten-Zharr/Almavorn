use super::{App, PlaybackQueue, PlaybackRequest};
use crate::app::session::{LocalSession, SavedEntry};
use anyhow::{Context, Result};

impl App {
    pub(in crate::app) fn local_session(&self) -> Option<LocalSession> {
        let pending = self.requested_playback();
        if pending.is_none() && (self.services_playing() || self.service_preparing()) {
            return None;
        }
        let queue = pending.map_or(&self.playback.queue, |r| &r.queue);
        let entry = queue.current()?;
        if pending.is_none() && self.playback.current.is_none() {
            return None;
        }
        Some(LocalSession {
            playlist: pending
                .map(|r| r.playlist_id)
                .or(self.playback.playing_playlist)?,
            current: SavedEntry {
                entry: entry.id,
                track: entry.track.id,
            },
            position_ms: pending.map_or_else(
                || self.position_ms(),
                |r| {
                    r.position_ms.unwrap_or_else(|| {
                        if r.reverse {
                            entry.track.duration_ms.saturating_sub(1)
                        } else {
                            0
                        }
                    })
                },
            ),
            paused: self.paused(),
            marked: queue.marked_only().then(|| {
                queue
                    .entries()
                    .iter()
                    .map(|entry| SavedEntry {
                        entry: entry.id,
                        track: entry.track.id,
                    })
                    .collect()
            }),
        })
    }

    pub(in crate::app) fn restore_local_session(&mut self, saved: LocalSession) -> Result<()> {
        // Resolve IDs against today's library: tag edits and physical moves may
        // have changed the pathname or moved the entry since the last checkpoint.
        let (playlist, current) = self
            .library
            .playlists
            .iter()
            .filter_map(|playlist| {
                let entry = playlist
                    .entries
                    .iter()
                    .filter(|entry| entry.track.id == saved.current.track)
                    .min_by_key(|entry| entry.id != saved.current.entry)?;
                Some((playlist, entry))
            })
            .min_by_key(|(playlist, entry)| {
                (
                    playlist.id != saved.playlist,
                    entry.id != saved.current.entry,
                )
            })
            .context(self.text(
                "The last played track is no longer in the library",
                "Последней прослушиваемой композиции больше нет в библиотеке",
            ))?;
        let marked_only = saved.marked.is_some();
        let marked: std::collections::HashSet<_> = saved
            .marked
            .iter()
            .flatten()
            .filter_map(|saved| {
                playlist
                    .entries
                    .iter()
                    .filter(|entry| entry.track.id == saved.track)
                    .min_by_key(|entry| entry.id != saved.entry)
                    .map(|entry| entry.id)
            })
            .collect();
        let queue: std::sync::Arc<[_]> = playlist
            .entries
            .iter()
            .filter(|entry| !marked_only || entry.id == current.id || marked.contains(&entry.id))
            .cloned()
            .collect();
        let index = queue
            .iter()
            .position(|entry| entry.id == current.id)
            .context("Saved playback queue is empty")?;
        let position = if current.track.duration_ms > 0 {
            saved
                .position_ms
                .min(current.track.duration_ms.saturating_sub(1))
        } else {
            saved.position_ms
        };
        let playlist_id = playlist.id;
        self.request_playback(PlaybackRequest {
            queue: PlaybackQueue::new(queue, index, marked_only),
            playlist_id,
            paused: saved.paused,
            reverse: false,
            position_ms: Some(position),
        })?;
        self.restore_playback_marks();
        Ok(())
    }
}
