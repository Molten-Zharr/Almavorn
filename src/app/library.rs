use super::{App, Focus, Sort, bounded};
use crate::model::{Entry, Mode, Playlist};
use anyhow::Result;
use std::collections::HashSet;

pub(super) struct RowsCache {
    revision: u64,
    playlist: i64,
    query: String,
    sort: Sort,
    descending: bool,
    indices: Vec<usize>,
}

fn sort_rows<T: Ord>(
    indices: &mut [usize],
    entries: &[Entry],
    descending: bool,
    key: impl Fn(&Entry) -> T,
) {
    if descending {
        indices.sort_by_cached_key(|&index| {
            (
                std::cmp::Reverse(key(&entries[index])),
                entries[index].position,
            )
        });
    } else {
        indices.sort_by_cached_key(|&index| (key(&entries[index]), entries[index].position));
    }
}

impl App {
    pub fn playlist(&self) -> Option<&Playlist> {
        self.library
            .playlists
            .iter()
            .find(|playlist| Some(playlist.id) == self.library.selected_playlist)
    }

    pub fn visible_playlists(&self) -> Vec<&Playlist> {
        self.library
            .playlists
            .iter()
            .filter(|playlist| playlist.mode == self.settings.mode)
            .collect()
    }

    pub fn playlists(&self) -> &[Playlist] {
        &self.library.playlists
    }

    pub fn rows(&self) -> Vec<&Entry> {
        let Some(playlist) = self.playlist() else {
            return Vec::new();
        };
        let mut cache = self.library.rows_cache.borrow_mut();
        let valid = cache.as_ref().is_some_and(|cache| {
            cache.revision == self.library.revision
                && cache.playlist == playlist.id
                && cache.query == self.library.query
                && cache.sort == self.library.sort
                && cache.descending == self.library.sort_descending
        });
        if !valid {
            let query = self.library.query.to_lowercase();
            let words: Vec<_> = query.split_whitespace().collect();
            let mut indices: Vec<_> = playlist
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    if words.is_empty() {
                        return true;
                    }
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
                .map(|(index, _)| index)
                .collect();
            let entries = &playlist.entries;
            let descending = self.library.sort_descending;
            match self.library.sort {
                Sort::Position => {
                    sort_rows(&mut indices, entries, descending, |entry| entry.position)
                }
                Sort::Title => sort_rows(&mut indices, entries, descending, |entry| {
                    entry.track.title.to_lowercase()
                }),
                Sort::Artist => sort_rows(&mut indices, entries, descending, |entry| {
                    entry.track.artist.to_lowercase()
                }),
                Sort::Album => sort_rows(&mut indices, entries, descending, |entry| {
                    entry.track.album.to_lowercase()
                }),
                Sort::Duration => sort_rows(&mut indices, entries, descending, |entry| {
                    entry.track.duration_ms
                }),
            }
            *cache = Some(RowsCache {
                revision: self.library.revision,
                playlist: playlist.id,
                query: self.library.query.clone(),
                sort: self.library.sort,
                descending,
                indices,
            });
        }
        cache
            .as_ref()
            .expect("rows cache was initialized")
            .indices
            .iter()
            .map(|&index| &playlist.entries[index])
            .collect()
    }

    pub fn entry(&self) -> Option<&Entry> {
        self.playlist()?
            .entries
            .iter()
            .find(|entry| Some(entry.id) == self.library.selected_entry)
    }

    pub(super) fn refresh(&mut self) -> Result<()> {
        if !self
            .visible_playlists()
            .iter()
            .any(|playlist| Some(playlist.id) == self.library.selected_playlist)
        {
            self.library.selected_playlist =
                self.visible_playlists().first().map(|playlist| playlist.id);
        }
        if !self
            .rows()
            .iter()
            .any(|entry| Some(entry.id) == self.library.selected_entry)
        {
            self.library.selected_entry = self.rows().first().map(|entry| entry.id);
        }
        let valid: HashSet<i64> = self
            .playlist()
            .map(|playlist| playlist.entries.iter().map(|entry| entry.id).collect())
            .unwrap_or_default();
        self.library.marked.retain(|id| valid.contains(id));
        Ok(())
    }

    pub(super) fn navigate(&mut self, direction: i64) {
        if self.view.focus == Focus::Playlists {
            let playlists = self.visible_playlists();
            let position = playlists
                .iter()
                .position(|playlist| Some(playlist.id) == self.library.selected_playlist)
                .unwrap_or(0);
            if !playlists.is_empty() {
                let id = playlists[bounded(position, direction, playlists.len())].id;
                self.select_playlist(id);
            }
        } else {
            let rows = self.rows();
            let position = rows
                .iter()
                .position(|entry| Some(entry.id) == self.library.selected_entry)
                .unwrap_or(0);
            if !rows.is_empty() {
                self.library.selected_entry =
                    Some(rows[bounded(position, direction, rows.len())].id);
            }
        }
    }

    pub(super) fn select_playlist(&mut self, id: i64) {
        self.library.selected_playlist = Some(id);
        self.library.selected_entry = self
            .playlist()
            .and_then(|playlist| playlist.entries.first())
            .map(|entry| entry.id);
        self.library.marked.clear();
        self.view.track_offset = 0;
    }

    pub fn set_mode(&mut self, mode: Mode) -> Result<()> {
        if self.busy() {
            return Ok(());
        }
        self.settings.mode = mode;
        self.view.editing = false;
        self.library.query.clear();
        self.library.marked.clear();
        self.library.selected_playlist =
            self.visible_playlists().first().map(|playlist| playlist.id);
        self.view.track_offset = 0;
        self.view.playlist_offset = 0;
        self.refresh()?;
        self.save_settings()
    }
}
