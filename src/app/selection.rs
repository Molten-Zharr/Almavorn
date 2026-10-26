use super::{App, Focus, Target};
use anyhow::Result;
use ratatui::layout::Position;
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub(super) struct MarkDrag {
    playlist: i64,
    last: i64,
    marked: bool,
    origin: Position,
    moved: bool,
    scrolled: Instant,
}

impl App {
    fn set_track_marks(&mut self, ids: &[i64], marked: bool) -> Result<()> {
        let mut changed = false;
        for id in ids {
            changed |= if marked {
                self.library.marked.insert(*id)
            } else {
                self.library.marked.remove(id)
            };
        }
        if changed {
            self.sync_marked_playback()?;
        }
        Ok(())
    }
    pub(super) fn start_mark_drag(&mut self, id: i64) -> Result<()> {
        let Some(playlist) = self.library.selected_playlist else {
            return Ok(());
        };
        if !self
            .playlist()
            .is_some_and(|p| p.entries.iter().any(|e| e.id == id))
        {
            return Ok(());
        }
        self.cancel_workspace_drag();
        let marked = !self.library.marked.contains(&id);
        self.set_track_marks(&[id], marked)?;
        self.view.mark_drag = Some(MarkDrag {
            playlist,
            last: id,
            marked,
            origin: self.view.pointer,
            moved: false,
            scrolled: Instant::now(),
        });
        Ok(())
    }
    fn paint_to(&mut self, id: i64) -> Result<()> {
        let Some(stroke) = &self.view.mark_drag else {
            return Ok(());
        };
        if self.library.selected_playlist != Some(stroke.playlist) || self.view.dialog.is_some() {
            self.view.mark_drag = None;
            return Ok(());
        }
        let rows = self.visible_track_rows();
        let Some(end) = rows.iter().position(|e| e.id == id) else {
            return Ok(());
        };
        let start = rows.iter().position(|e| e.id == stroke.last).unwrap_or(end);
        let ids = rows[start.min(end)..=start.max(end)]
            .iter()
            .map(|e| e.id)
            .collect::<Vec<_>>();
        let marked = stroke.marked;
        self.set_track_marks(&ids, marked)?;
        if let Some(stroke) = &mut self.view.mark_drag {
            stroke.last = id;
        }
        self.library.selected_entry = Some(id);
        self.view.track_scroll_selected = None;
        self.view.focus = Focus::Tracks;
        Ok(())
    }
    pub(super) fn drag_track_marks(&mut self, point: Position) -> Result<bool> {
        let Some(stroke) = &mut self.view.mark_drag else {
            return Ok(false);
        };
        stroke.moved |= point != stroke.origin;
        if let Some(Target::Track(id) | Target::Mark(id)) = self.target_at(point).cloned() {
            self.paint_to(id)?;
        }
        Ok(true)
    }
    pub(super) fn tick_mark_drag(&mut self) -> Result<()> {
        let Some(stroke) = &self.view.mark_drag else {
            return Ok(());
        };
        if self.view.dialog.is_some() || self.library.selected_playlist != Some(stroke.playlist) {
            self.view.mark_drag = None;
            return Ok(());
        }
        if !stroke.moved
            || stroke.scrolled.elapsed() < Duration::from_millis(80)
            || !self.view.tracks_area.contains(self.view.pointer)
        {
            return Ok(());
        }
        let mut rows = self
            .view
            .hits
            .iter()
            .filter(|hit| hit.enabled && matches!(hit.target, Target::Mark(_)))
            .map(|hit| hit.area.y);
        let Some(first) = rows.next() else {
            return Ok(());
        };
        let (first, last) = rows.fold((first, first), |(min, max), y| (min.min(y), max.max(y)));
        let direction = if self.view.pointer.y <= first {
            -1
        } else if self.view.pointer.y >= last {
            1
        } else {
            return Ok(());
        };
        let entries = self.visible_track_rows();
        let Some(index) = entries.iter().position(|e| e.id == stroke.last) else {
            return Ok(());
        };
        let next = super::bounded(index, direction, entries.len());
        let id = entries[next].id;
        if let Some(stroke) = &mut self.view.mark_drag {
            stroke.scrolled = Instant::now();
        }
        self.paint_to(id)
    }
    pub(super) fn mark_folder_group(&mut self, folder: &Path) -> Result<()> {
        if !self.playlist().is_some_and(|p| p.groups_by_folder()) {
            return Ok(());
        }
        let ids = self
            .rows()
            .into_iter()
            .filter(|e| e.track.path.parent().unwrap_or(Path::new("")) == folder)
            .map(|e| e.id)
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let marked = !ids.iter().all(|id| self.library.marked.contains(id));
        self.set_track_marks(&ids, marked)?;
        self.library.selected_entry = ids.first().copied();
        self.view.track_scroll_selected = None;
        self.view.focus = Focus::Tracks;
        Ok(())
    }
}
