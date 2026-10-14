use crate::model::{Entry, RepeatMode};
use std::{collections::VecDeque, sync::Arc};

const SHUFFLE_HISTORY_LIMIT: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QueueStep {
    Previous,
    Next,
    Finished,
}

impl QueueStep {
    pub fn manual(direction: i64) -> Self {
        if direction < 0 {
            Self::Previous
        } else {
            Self::Next
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QueueUpdate {
    Empty,
    Kept,
    Replaced,
}

enum Transition {
    Replay,
    Ordered(usize),
    ShuffleForward,
    ShuffleBack(usize),
}

/// A queue snapshot, independent of the library view and audio preparation.
/// Repeat and shuffle are supplied at each transition so pending queues also
/// follow the latest settings. Clones share entries but own their cursor/history.
#[derive(Clone, Default)]
pub(super) struct PlaybackQueue {
    entries: Arc<[Entry]>,
    index: usize,
    history: VecDeque<usize>,
    marked_only: bool,
}

impl PlaybackQueue {
    pub fn new(entries: Arc<[Entry]>, index: usize, marked_only: bool) -> Self {
        Self {
            entries,
            index,
            marked_only,
            history: VecDeque::new(),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.index)
    }

    pub fn marked_only(&self) -> bool {
        self.marked_only
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn can_step(&self, step: QueueStep, repeat: RepeatMode, shuffle: bool) -> bool {
        // Previous in shuffle mode requires history, including a single marked
        // entry which otherwise loops on direct advances and completion.
        if step == QueueStep::Previous && shuffle && self.history.is_empty() {
            return false;
        }
        self.transition(step, repeat, shuffle).is_some()
    }

    pub fn advance(&self, step: QueueStep, repeat: RepeatMode, shuffle: bool) -> Option<Self> {
        let transition = self.transition(step, repeat, shuffle)?;
        let mut next = self.clone();
        match transition {
            Transition::Replay => {}
            Transition::Ordered(index) => {
                next.index = index;
                next.history.clear();
            }
            Transition::ShuffleForward => {
                // Uniformly sample every position except the current one.
                let choice = fastrand::usize(..self.entries.len() - 1);
                next.index = choice + usize::from(choice >= self.index);
                next.history.push_back(self.index);
                if next.history.len() > SHUFFLE_HISTORY_LIMIT {
                    next.history.pop_front();
                }
            }
            Transition::ShuffleBack(index) => {
                next.index = index;
                next.history.pop_back();
            }
        }
        Some(next)
    }

    fn transition(&self, step: QueueStep, repeat: RepeatMode, shuffle: bool) -> Option<Transition> {
        self.current()?;
        if (!self.marked_only && step == QueueStep::Finished && repeat == RepeatMode::Track)
            || (self.marked_only && self.entries.len() == 1)
        {
            return Some(Transition::Replay);
        }
        if shuffle {
            return if step == QueueStep::Previous {
                self.history.back().copied().map(Transition::ShuffleBack)
            } else {
                (self.entries.len() > 1).then_some(Transition::ShuffleForward)
            };
        }
        let wraps = self.marked_only || repeat == RepeatMode::Playlist;
        let index = if step == QueueStep::Previous {
            self.index
                .checked_sub(1)
                .or_else(|| wraps.then_some(self.entries.len() - 1))
        } else if self.index + 1 < self.entries.len() {
            Some(self.index + 1)
        } else {
            wraps.then_some(0)
        };
        index.map(Transition::Ordered)
    }

    /// Retain the requested source when its entry and path still exist. If it
    /// disappeared, select the next stored position, wrapping to the first row.
    pub fn replace(&mut self, entries: Arc<[Entry]>, marked_only: bool) -> QueueUpdate {
        if entries.is_empty() {
            *self = Self::default();
            return QueueUpdate::Empty;
        }
        let current = self.current();
        let kept = current.and_then(|current| {
            entries
                .iter()
                .position(|entry| entry.id == current.id && entry.track.path == current.track.path)
        });
        let index = kept.unwrap_or_else(|| {
            current
                .and_then(|current| {
                    entries
                        .iter()
                        .position(|entry| entry.position > current.position)
                })
                .unwrap_or(0)
        });
        self.retarget(entries, index, marked_only);
        if kept.is_some() {
            QueueUpdate::Kept
        } else {
            QueueUpdate::Replaced
        }
    }

    /// Rebase history while retaining a separately identified audible entry.
    pub fn retarget(&mut self, entries: Arc<[Entry]>, index: usize, marked_only: bool) {
        self.history = self
            .history
            .iter()
            .filter_map(|&index| {
                let id = self.entries.get(index)?.id;
                entries.iter().position(|entry| entry.id == id)
            })
            .collect();
        self.entries = entries;
        self.index = index;
        self.marked_only = marked_only;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Track;

    fn queue(length: usize, index: usize, marked_only: bool) -> PlaybackQueue {
        let entries = (0..length)
            .map(|index| Entry {
                id: index as i64 + 1,
                position: index as i64,
                track: Track {
                    id: index as i64 + 1,
                    path: format!("track-{index}.wav").into(),
                    title: format!("Track {index}"),
                    artist: String::new(),
                    album: String::new(),
                    duration_ms: 1000,
                    tags: "{}".into(),
                },
            })
            .collect();
        PlaybackQueue::new(entries, index, marked_only)
    }

    fn id(queue: &PlaybackQueue) -> i64 {
        queue.current().unwrap().id
    }

    #[test]
    fn ordered_steps_stop_at_the_edges_and_track_repeat_only_intercepts_completion() {
        let current = queue(3, 0, false);
        assert!(!current.can_step(QueueStep::Previous, RepeatMode::Off, false));
        assert!(
            current
                .advance(QueueStep::Previous, RepeatMode::Off, false)
                .is_none()
        );
        let current = current
            .advance(QueueStep::Next, RepeatMode::Off, false)
            .unwrap();
        assert_eq!(id(&current), 2);
        for shuffle in [false, true] {
            assert_eq!(
                id(&current
                    .advance(QueueStep::Finished, RepeatMode::Track, shuffle)
                    .unwrap()),
                2,
            );
            assert_ne!(
                id(&current
                    .advance(QueueStep::Next, RepeatMode::Track, shuffle)
                    .unwrap()),
                2,
            );
        }
        let last = current
            .advance(QueueStep::Next, RepeatMode::Off, false)
            .unwrap();
        assert_eq!(id(&last), 3);
        assert!(!last.can_step(QueueStep::Next, RepeatMode::Track, false));
        assert!(
            last.advance(QueueStep::Next, RepeatMode::Track, false)
                .is_none()
        );
        assert!(
            last.advance(QueueStep::Finished, RepeatMode::Off, false)
                .is_none()
        );
    }

    #[test]
    fn playlist_repeat_and_marked_sets_wrap_in_both_directions() {
        for (marked_only, repeat) in [(false, RepeatMode::Playlist), (true, RepeatMode::Track)] {
            let last = queue(3, 2, marked_only);
            assert!(last.can_step(QueueStep::Next, repeat, false));
            let first = last.advance(QueueStep::Finished, repeat, false).unwrap();
            assert_eq!(id(&first), 1);
            assert!(first.can_step(QueueStep::Previous, repeat, false));
            assert_eq!(
                id(&first.advance(QueueStep::Previous, repeat, false).unwrap()),
                3
            );
            if !marked_only {
                assert!(!first.can_step(QueueStep::Previous, RepeatMode::Off, false));
                assert!(
                    first
                        .advance(QueueStep::Previous, RepeatMode::Off, false)
                        .is_none()
                );
            }
        }
    }

    #[test]
    fn empty_invalid_and_single_entry_queues_handle_every_playback_mode() {
        for shuffle in [false, true] {
            for repeat in [RepeatMode::Off, RepeatMode::Playlist, RepeatMode::Track] {
                for marked_only in [false, true] {
                    for current in [queue(0, 0, marked_only), queue(1, 1, marked_only)] {
                        for step in [QueueStep::Previous, QueueStep::Next, QueueStep::Finished] {
                            assert!(!current.can_step(step, repeat, shuffle));
                            assert!(current.advance(step, repeat, shuffle).is_none());
                        }
                    }
                    let current = queue(1, 0, marked_only);
                    let completes = marked_only
                        || repeat == RepeatMode::Track
                        || (!shuffle && repeat == RepeatMode::Playlist);
                    assert_eq!(
                        current
                            .advance(QueueStep::Finished, repeat, shuffle)
                            .is_some(),
                        completes
                    );
                    let skips = marked_only || (!shuffle && repeat == RepeatMode::Playlist);
                    assert_eq!(current.can_step(QueueStep::Next, repeat, shuffle), skips);
                    assert_eq!(
                        current.advance(QueueStep::Next, repeat, shuffle).is_some(),
                        skips
                    );
                    assert_eq!(
                        current.can_step(QueueStep::Previous, repeat, shuffle),
                        skips && !shuffle
                    );
                    assert_eq!(
                        current
                            .advance(QueueStep::Previous, repeat, shuffle)
                            .is_some(),
                        skips
                    );
                }
            }
        }
    }

    #[test]
    fn shuffle_excludes_the_current_entry_and_previous_retraces_actual_history() {
        fastrand::seed(7);
        let mut current = queue(5, 4, false);
        assert!(!current.can_step(QueueStep::Previous, RepeatMode::Off, true));
        let mut visited = vec![id(&current)];
        for _ in 0..128 {
            assert!(current.can_step(QueueStep::Next, RepeatMode::Off, true));
            let next = current
                .advance(QueueStep::Finished, RepeatMode::Off, true)
                .unwrap();
            assert_ne!(id(&next), id(&current));
            current = next;
            visited.push(id(&current));
        }
        assert!((1..=5).all(|id| visited.contains(&id)));
        for &expected in visited[..visited.len() - 1].iter().rev() {
            assert!(current.can_step(QueueStep::Previous, RepeatMode::Off, true));
            current = current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .unwrap();
            assert_eq!(id(&current), expected);
        }
        assert!(!current.can_step(QueueStep::Previous, RepeatMode::Off, true));
        assert!(
            current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .is_none()
        );
    }

    #[test]
    fn shuffle_history_is_bounded_and_availability_does_not_consume_random_choices() {
        let initial = queue(2, 0, false);
        let mut current = initial.clone();
        for _ in 0..SHUFFLE_HISTORY_LIMIT + 10 {
            current = current
                .advance(QueueStep::Next, RepeatMode::Off, true)
                .unwrap();
        }
        for _ in 0..SHUFFLE_HISTORY_LIMIT {
            current = current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .unwrap();
        }
        assert!(
            current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .is_none()
        );

        let mut current = initial
            .advance(QueueStep::Next, RepeatMode::Off, true)
            .unwrap();
        assert!(current.can_step(QueueStep::Previous, RepeatMode::Off, true));
        current.clear_history();
        assert!(!current.can_step(QueueStep::Previous, RepeatMode::Off, true));

        let initial = queue(5, 0, false);
        fastrand::seed(7);
        let expected = id(&initial
            .advance(QueueStep::Next, RepeatMode::Off, true)
            .unwrap());
        fastrand::seed(7);
        for _ in 0..10 {
            assert!(initial.can_step(QueueStep::Next, RepeatMode::Off, true));
        }
        assert_eq!(
            id(&initial
                .advance(QueueStep::Next, RepeatMode::Off, true)
                .unwrap()),
            expected
        );
    }

    #[test]
    fn track_repeat_completion_keeps_shuffle_history_for_manual_previous() {
        let first = queue(2, 0, false);
        let second = first
            .advance(QueueStep::Next, RepeatMode::Track, true)
            .unwrap();
        assert_eq!(id(&second), 2);
        let repeated = second
            .advance(QueueStep::Finished, RepeatMode::Track, true)
            .unwrap();
        assert_eq!(id(&repeated), 2);
        assert!(repeated.can_step(QueueStep::Previous, RepeatMode::Track, true));
        assert_eq!(
            id(&repeated
                .advance(QueueStep::Previous, RepeatMode::Track, true)
                .unwrap()),
            1
        );
    }

    #[test]
    fn replacing_entries_preserves_the_source_or_selects_its_stored_successor() {
        let mut current = queue(4, 1, true);
        let entries = current.entries().to_vec();
        assert_eq!(
            current.replace(vec![entries[3].clone(), entries[1].clone()].into(), true),
            QueueUpdate::Kept
        );
        assert_eq!(id(&current), 2);
        assert_eq!(
            current.replace(
                vec![entries[0].clone(), entries[2].clone(), entries[3].clone()].into(),
                true
            ),
            QueueUpdate::Replaced
        );
        assert_eq!(id(&current), 3);
        assert_eq!(
            current.replace(vec![entries[0].clone()].into(), true),
            QueueUpdate::Replaced
        );
        assert_eq!(id(&current), 1);

        let mut changed_path = entries[0].clone();
        changed_path.track.path = "replacement.wav".into();
        assert_eq!(
            current.replace(vec![changed_path].into(), false),
            QueueUpdate::Replaced
        );
        assert!(!current.marked_only());
        assert_eq!(current.replace(Arc::from([]), false), QueueUpdate::Empty);
        assert!(current.current().is_none());
        assert!(!current.can_step(QueueStep::Next, RepeatMode::Playlist, true));
    }

    #[test]
    fn replacing_entries_remaps_shuffle_history_by_identity_and_drops_removed_entries() {
        fastrand::seed(7);
        let mut current = queue(3, 0, true);
        let original = current.entries().to_vec();
        let mut history = Vec::new();
        for _ in 0..20 {
            history.push(id(&current));
            current = current
                .advance(QueueStep::Next, RepeatMode::Off, true)
                .unwrap();
        }
        current.replace(vec![original[2].clone(), original[0].clone()].into(), true);
        for expected in history.into_iter().rev().filter(|&id| id != 2) {
            current = current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .unwrap();
            assert_eq!(id(&current), expected);
        }
        assert!(!current.can_step(QueueStep::Previous, RepeatMode::Off, true));
        current.clear_history();
        assert!(
            current
                .advance(QueueStep::Previous, RepeatMode::Off, true)
                .is_none()
        );
    }
}
