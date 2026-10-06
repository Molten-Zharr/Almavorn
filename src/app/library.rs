use super::{App, Focus, Sort, bounded};
use crate::model::{Entry, Mode, Playlist};
use anyhow::Result;
use std::collections::HashSet;

impl App {
    pub fn playlist(&self) -> Option<&Playlist> {
        self.playlists
            .iter()
            .find(|playlist| Some(playlist.id) == self.selected_playlist)
    }

    pub fn visible_playlists(&self) -> Vec<&Playlist> {
        self.playlists
            .iter()
            .filter(|playlist| playlist.mode == self.settings.mode)
            .collect()
    }

    pub fn rows(&self) -> Vec<&Entry> {
        let Some(playlist) = self.playlist() else {
            return Vec::new();
        };
        let query = self.query.to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut entries: Vec<_> = playlist
            .entries
            .iter()
            .filter(|entry| {
                let text = format!(
                    "{} {} {} {}",
                    entry.track.title,
                    entry.track.artist,
                    entry.track.album,
                    entry.track.path.display()
                )
                .to_lowercase();
                words.iter().all(|word| text.contains(word))
            })
            .collect();
        entries.sort_by(|a, b| {
            let order = match self.sort {
                Sort::Position => a.position.cmp(&b.position),
                Sort::Title => a
                    .track
                    .title
                    .to_lowercase()
                    .cmp(&b.track.title.to_lowercase()),
                Sort::Artist => a
                    .track
                    .artist
                    .to_lowercase()
                    .cmp(&b.track.artist.to_lowercase()),
                Sort::Album => a
                    .track
                    .album
                    .to_lowercase()
                    .cmp(&b.track.album.to_lowercase()),
                Sort::Duration => a.track.duration_ms.cmp(&b.track.duration_ms),
            };
            (if self.sort_descending {
                order.reverse()
            } else {
                order
            })
            .then_with(|| a.position.cmp(&b.position))
        });
        entries
    }

    pub fn entry(&self) -> Option<&Entry> {
        self.playlist()?
            .entries
            .iter()
            .find(|entry| Some(entry.id) == self.selected_entry)
    }

    pub(super) fn refresh(&mut self) -> Result<()> {
        if !self
            .visible_playlists()
            .iter()
            .any(|playlist| Some(playlist.id) == self.selected_playlist)
        {
            self.selected_playlist = self.visible_playlists().first().map(|playlist| playlist.id);
        }
        if !self
            .rows()
            .iter()
            .any(|entry| Some(entry.id) == self.selected_entry)
        {
            self.selected_entry = self.rows().first().map(|entry| entry.id);
        }
        let valid: HashSet<i64> = self
            .playlist()
            .map(|playlist| playlist.entries.iter().map(|entry| entry.id).collect())
            .unwrap_or_default();
        self.marked.retain(|id| valid.contains(id));
        Ok(())
    }

    pub(super) fn navigate(&mut self, direction: i64) {
        if self.focus == Focus::Playlists {
            let playlists = self.visible_playlists();
            let position = playlists
                .iter()
                .position(|playlist| Some(playlist.id) == self.selected_playlist)
                .unwrap_or(0);
            if !playlists.is_empty() {
                let id = playlists[bounded(position, direction, playlists.len())].id;
                self.select_playlist(id);
            }
        } else {
            let rows = self.rows();
            let position = rows
                .iter()
                .position(|entry| Some(entry.id) == self.selected_entry)
                .unwrap_or(0);
            if !rows.is_empty() {
                self.selected_entry = Some(rows[bounded(position, direction, rows.len())].id);
            }
        }
    }

    pub(super) fn select_playlist(&mut self, id: i64) {
        self.selected_playlist = Some(id);
        self.selected_entry = self
            .playlist()
            .and_then(|playlist| playlist.entries.first())
            .map(|entry| entry.id);
        self.marked.clear();
        self.track_offset = 0;
    }

    pub fn set_mode(&mut self, mode: Mode) -> Result<()> {
        if self.busy() {
            return Ok(());
        }
        self.settings.mode = mode;
        self.editing = false;
        self.query.clear();
        self.marked.clear();
        self.selected_playlist = self.visible_playlists().first().map(|playlist| playlist.id);
        self.track_offset = 0;
        self.playlist_offset = 0;
        self.refresh()?;
        self.save_settings()
    }
}
