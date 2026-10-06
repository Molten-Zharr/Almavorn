use super::App;
use crate::{
    model::{Mode, PlaylistKind},
    store::{Change, Snapshot},
};
use anyhow::Result;

impl App {
    pub(super) fn scope(&self) -> Option<&'static str> {
        if self
            .playlist()
            .is_some_and(|playlist| playlist.kind == PlaylistKind::SortingDesk)
        {
            Some("desk")
        } else if self.settings.mode == Mode::Chaos {
            Some("chaos")
        } else {
            None
        }
    }

    pub(super) fn remember(&mut self, change: Change) -> Result<()> {
        if change.scope != "order" && change.before != change.after {
            let history = self.library.histories.entry(change.scope).or_default();
            history.1.clear();
            // Results can arrive in a different order from commits. Keep only a connected history.
            if history
                .0
                .last()
                .is_some_and(|previous| previous.after != change.before)
            {
                history.0.clear();
            }
            history.0.push(change);
            if history.0.len() > 100 {
                history.0.remove(0);
            }
        }
        Ok(())
    }

    pub(super) fn validate_history(&mut self) {
        for (scope, (undo, redo)) in &mut self.library.histories {
            let expected = undo
                .last()
                .map(|change| &change.after)
                .or_else(|| redo.last().map(|change| &change.before));
            if expected.is_some_and(|expected| {
                *expected != Snapshot::from_library(&self.library.playlists, scope)
            }) {
                undo.clear();
                redo.clear();
            }
        }
    }
}
