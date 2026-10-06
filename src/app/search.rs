use super::{App, Dialog, background::background, bounded};
use crate::{
    errors::AppError,
    input::{Key, KeyPress},
    model::{Entry, Language, Playlist},
    workspace::Panel,
};
use anyhow::{Context, Result};
use ratatui::layout::Rect;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchFocus {
    #[default]
    Input,
    Results,
    Playlists,
}

#[derive(Clone, Copy)]
pub(crate) struct SearchHit {
    playlist: usize,
    entry: usize,
}

struct SearchItem {
    hit: SearchHit,
    playlist_id: i64,
    text: String,
}

#[derive(Clone)]
pub struct SearchDialog {
    pub query: String,
    pub playlists: HashSet<i64>,
    pub focus: SearchFocus,
    pub selected: usize,
    pub playlist_selected: usize,
    pub(crate) offset: usize,
    pub(crate) playlist_offset: usize,
    pub(crate) results: Vec<SearchHit>,
    pub(crate) loading: bool,
    pub(crate) failed: bool,
    pub(crate) selected_all: bool,
    pub(crate) keyboard_visible: bool,
    pub(crate) keyboard: Language,
    pub(crate) upper: bool,
    pub(crate) results_area: Rect,
    pub(crate) playlists_area: Rect,
    revision: Option<u64>,
    index: Arc<[SearchItem]>,
    requested: Option<(String, HashSet<i64>)>,
}

pub(super) struct SearchRequest {
    index: Arc<[SearchItem]>,
    query: String,
    playlists: HashSet<i64>,
}

impl App {
    pub(super) fn open_search(&mut self) {
        self.cancel_workspace_drag();
        self.view.filter_editing = false;
        self.view.filter_keyboard = false;
        self.search_job.cancel();
        self.view.dialog = Some(Dialog::Search(SearchDialog {
            query: String::new(),
            playlists: self.playlists().iter().map(|p| p.id).collect(),
            focus: SearchFocus::Input,
            selected: 0,
            playlist_selected: 0,
            offset: 0,
            playlist_offset: 0,
            results: Vec::new(),
            loading: false,
            failed: false,
            selected_all: false,
            keyboard_visible: false,
            keyboard: self.settings.language,
            upper: false,
            results_area: Rect::default(),
            playlists_area: Rect::default(),
            revision: None,
            index: Arc::from([]),
            requested: None,
        }));
    }

    pub(super) fn tick_search(&mut self) -> Result<()> {
        if !matches!(self.view.dialog, Some(Dialog::Search(_))) {
            if self.search_job.requested().is_some() {
                self.search_job.cancel();
            }
            let _ = self.search_job.poll();
            return Ok(());
        }
        let Some(Dialog::Search(mut search)) = self.view.dialog.take() else {
            // Take only Search; other dialogs keep their normal ownership.
            unreachable!("tick_search is called only for the search dialog");
        };
        if search.revision != Some(self.library.revision) {
            search
                .playlists
                .retain(|id| self.playlists().iter().any(|p| p.id == *id));
            search.index = self
                .playlists()
                .iter()
                .enumerate()
                .flat_map(|(playlist, p)| {
                    p.entries
                        .iter()
                        .enumerate()
                        .map(move |(entry, e)| SearchItem {
                            hit: SearchHit { playlist, entry },
                            playlist_id: p.id,
                            text: format!(
                                "{} {} {} {}",
                                e.track.title,
                                e.track.artist,
                                e.track.album,
                                e.track.path.display()
                            )
                            .to_lowercase(),
                        })
                })
                .collect::<Vec<_>>()
                .into();
            search.revision = Some(self.library.revision);
            search.requested = None;
        }
        if search.requested.as_ref().is_none_or(|(query, playlists)| {
            query != &search.query || playlists != &search.playlists
        }) {
            search.requested = Some((search.query.clone(), search.playlists.clone()));
            search.results.clear();
            search.selected = 0;
            search.offset = 0;
            search.loading = true;
            search.failed = false;
            self.search_job.request(SearchRequest {
                index: search.index.clone(),
                query: search.query.clone(),
                playlists: search.playlists.clone(),
            });
        }
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.search_job.start(|request| {
            let index = request.index.clone();
            let query = request.query.to_lowercase();
            let playlists = request.playlists.clone();
            background(runtime, move || {
                let words: Vec<_> = query.split_whitespace().collect();
                Ok(index
                    .iter()
                    .filter(|item| {
                        playlists.contains(&item.playlist_id)
                            && words.iter().all(|word| item.text.contains(word))
                    })
                    .map(|item| item.hit)
                    .collect())
            })
        });
        let mut result = Ok(());
        if let Some(completed) = self.search_job.poll()
            && completed.current
        {
            search.loading = false;
            match completed.result {
                Ok(results) => search.results = results,
                Err(error) => {
                    search.failed = true;
                    result = Err(error);
                }
            }
        }
        search.playlist_selected = search
            .playlist_selected
            .min(self.playlists().len().saturating_sub(1));
        self.view.dialog = Some(Dialog::Search(search));
        result
    }

    pub(crate) fn search_entry(&self, hit: SearchHit) -> Option<(&Playlist, &Entry)> {
        let playlist = self.playlists().get(hit.playlist)?;
        Some((playlist, playlist.entries.get(hit.entry)?))
    }

    pub(super) fn play_search_result(&mut self) -> Result<()> {
        let hit = match &self.view.dialog {
            Some(Dialog::Search(search)) => search.results.get(search.selected).copied(),
            _ => None,
        }
        .context(AppError::TrackSelectionRequired)?;
        let (playlist, entry) = self.search_entry(hit).context(AppError::TrackMissing)?;
        let playlist_id = playlist.id;
        let index = playlist
            .entries
            .iter()
            .position(|e| e.id == entry.id)
            .context(AppError::TrackMissing)?;
        self.prepare_playback(playlist.entries.clone().into(), playlist_id, index)
    }

    pub(super) fn search_navigate(&mut self, direction: i64) {
        let length = self.playlists().len();
        if let Some(Dialog::Search(search)) = &mut self.view.dialog {
            if search.focus == SearchFocus::Playlists {
                search.playlist_selected = bounded(search.playlist_selected, direction, length);
            } else {
                search.focus = SearchFocus::Results;
                search.selected = bounded(search.selected, direction, search.results.len());
            }
        }
    }

    pub(super) fn search_key(&mut self, key: KeyPress) -> Result<()> {
        let playlist_id = if let Some(Dialog::Search(search)) = &self.view.dialog {
            self.playlists().get(search.playlist_selected).map(|p| p.id)
        } else {
            None
        };
        let Some(Dialog::Search(search)) = &mut self.view.dialog else {
            return Ok(());
        };
        // Result navigation never takes text input away from the query field.
        if matches!(
            key.key,
            Key::Up | Key::Down | Key::PageUp | Key::PageDown | Key::Home | Key::End
        ) {
            search.focus = SearchFocus::Results;
        }
        match key.key {
            Key::Tab => {
                search.focus = match (search.focus, key.shift) {
                    (SearchFocus::Input, false) | (SearchFocus::Playlists, true) => {
                        SearchFocus::Results
                    }
                    (SearchFocus::Results, false) | (SearchFocus::Input, true) => {
                        SearchFocus::Playlists
                    }
                    _ => SearchFocus::Input,
                }
            }
            Key::Char('l') if key.ctrl => {
                self.target(super::Target::SearchAll(true), false)?
            }
            Key::Char('a') if key.ctrl => {
                search.focus = SearchFocus::Input;
                search.selected_all = true;
            }
            Key::Char('d') if key.ctrl && search.focus == SearchFocus::Playlists => {
                self.target(super::Target::SearchAll(false), false)?
            }
            Key::Backspace => {
                search.focus = SearchFocus::Input;
                if search.selected_all {
                    search.query.clear();
                    search.selected_all = false;
                } else {
                    search.query.pop();
                }
            }
            Key::Up => self.search_navigate(-1),
            Key::Down => self.search_navigate(1),
            Key::PageUp => self.search_navigate(-8),
            Key::PageDown => self.search_navigate(8),
            Key::Home => self.search_navigate(i64::MIN),
            Key::End => self.search_navigate(i64::MAX),
            Key::Left if search.focus == SearchFocus::Playlists => self.search_navigate(-1),
            Key::Right if search.focus == SearchFocus::Playlists => self.search_navigate(1),
            Key::Char(' ') if key.ctrl && search.focus == SearchFocus::Playlists => {
                if let Some(id) = playlist_id {
                    self.toggle_search_playlist(id);
                }
            }
            Key::Enter if !search.results.is_empty() => self.play_search_result()?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn toggle_search_playlist(&mut self, id: i64) {
        let index = self.playlists().iter().position(|p| p.id == id);
        if let Some(Dialog::Search(search)) = &mut self.view.dialog
            && let Some(index) = index
        {
            search.focus = SearchFocus::Playlists;
            search.playlist_selected = index;
            if !search.playlists.insert(id) {
                search.playlists.remove(&id);
            }
        }
    }

    pub(super) fn edit_filter(&mut self) -> Result<()> {
        self.cancel_workspace_drag();
        if self.view.filter_editing {
            self.view.filter_editing = false;
            self.view.filter_keyboard = false;
            return Ok(());
        }
        let hidden = self.settings.workspace.hidden.contains(&Panel::Tracks)
            || self.settings.workspace.collapsed.contains(&Panel::Tracks);
        self.settings
            .workspace
            .hidden
            .retain(|p| *p != Panel::Tracks);
        self.settings
            .workspace
            .collapsed
            .retain(|p| *p != Panel::Tracks);
        if hidden {
            self.save_settings()?;
        }
        self.view.filter_before = self.library.query.clone();
        self.view.filter_selected_all = !self.library.query.is_empty();
        self.view.filter_keyboard_language = self.settings.language;
        self.view.filter_editing = true;
        self.focus_panel(Panel::Tracks);
        Ok(())
    }

    pub(super) fn filter_key(&mut self, key: KeyPress) -> Result<()> {
        if self.settings.bindings.iter().any(|binding| {
            binding.action == crate::input::Action::Filter && binding.key == key.normalized()
        }) {
            self.edit_filter()?;
            return Ok(());
        }
        match key.key {
            Key::Escape => {
                self.library.query = self.view.filter_before.clone();
                self.view.filter_editing = false;
                self.view.filter_keyboard = false;
                self.refresh()?;
            }
            Key::Enter | Key::Tab => {
                self.view.filter_editing = false;
                self.view.filter_keyboard = false;
            }
            Key::Char('a') if key.ctrl => self.view.filter_selected_all = true,
            Key::Backspace => {
                if self.view.filter_selected_all {
                    self.library.query.clear();
                    self.view.filter_selected_all = false;
                } else {
                    self.library.query.pop();
                }
                self.refresh()?;
            }
            Key::Up => self.navigate(-1),
            Key::Down => self.navigate(1),
            Key::PageUp => self.navigate(-8),
            Key::PageDown => self.navigate(8),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn query_text(&mut self, text: &str) -> Result<bool> {
        let input = text.chars().filter(|c| !c.is_control());
        if let Some(Dialog::Search(search)) = &mut self.view.dialog {
            search.focus = SearchFocus::Input;
            if search.selected_all {
                search.query.clear();
                search.selected_all = false;
            }
            let remaining = 512usize.saturating_sub(search.query.chars().count());
            search.query.extend(input.take(remaining));
            return Ok(true);
        }
        if self.view.dialog.is_none() && self.view.filter_editing {
            if self.view.filter_selected_all {
                self.library.query.clear();
                self.view.filter_selected_all = false;
            }
            let remaining = 512usize.saturating_sub(self.library.query.chars().count());
            self.library.query.extend(input.take(remaining));
            self.view.track_offset = 0;
            self.refresh()?;
            return Ok(true);
        }
        Ok(false)
    }
}
