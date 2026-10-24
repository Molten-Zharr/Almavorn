use super::{
    App, Dialog, Target,
    background::{Background, background, poll},
    bounded,
};
use crate::{
    input::{Action, Input, Key, KeyPress},
    model::{Language, PlaylistKind},
    statistics::{ListeningRecord, ListeningStatistics, ListeningTracker, StatisticsRow},
    workspace::Panel,
};
use anyhow::{Context, Result};
use ratatui::layout::Position;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::runtime::Runtime;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatisticsPage {
    #[default]
    Tracks,
    Artists,
    Albums,
    Recent,
}

impl StatisticsPage {
    pub const ALL: [Self; 4] = [Self::Tracks, Self::Artists, Self::Albums, Self::Recent];
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Tracks => language.text("Tracks", "Треки"),
            Self::Artists => language.text("Artists", "Артисты"),
            Self::Albums => language.text("Albums", "Альбомы"),
            Self::Recent => language.text("Recent", "Недавние"),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatisticsSort {
    #[default]
    Time,
    Plays,
    Starts,
    Name,
}
impl StatisticsSort {
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Time => language.text("Time", "Время"),
            Self::Plays => language.text("Plays", "Прослушивания"),
            Self::Starts => language.text("Starts", "Включения"),
            Self::Name => language.text("Name", "Название"),
        }
    }
}

#[derive(Default)]
pub struct StatisticsView {
    pub snapshot: ListeningStatistics,
    pub rows: Vec<StatisticsRow>,
    pub page: StatisticsPage,
    pub sort: StatisticsSort,
    pub selected: usize,
    pub offset: usize,
    pub capacity: usize,
    pub loading: bool,
    pub error: Option<String>,
    pub focused: bool,
}
impl StatisticsView {
    fn rebuild(&mut self) {
        let selected = self
            .rows
            .get(self.selected)
            .and_then(|row| row.track_id.map(|id| (id, row.date.clone())));
        self.rows = match self.page {
            StatisticsPage::Tracks => &self.snapshot.tracks,
            StatisticsPage::Artists => &self.snapshot.artists,
            StatisticsPage::Albums => &self.snapshot.albums,
            StatisticsPage::Recent => &self.snapshot.recent,
        }
        .clone();
        if self.page != StatisticsPage::Recent {
            let sort = self.sort;
            self.rows.sort_by_cached_key(|row| match sort {
                StatisticsSort::Time => (
                    u64::MAX - row.listened_ms,
                    u64::MAX - row.plays,
                    row.title.to_lowercase(),
                ),
                StatisticsSort::Plays => (
                    u64::MAX - row.plays,
                    u64::MAX - row.listened_ms,
                    row.title.to_lowercase(),
                ),
                StatisticsSort::Starts => (u64::MAX-row.starts,u64::MAX-row.listened_ms,row.title.to_lowercase()),
                StatisticsSort::Name => (0, 0, row.title.to_lowercase()),
            });
        }
        self.selected = selected
            .and_then(|(id, date)| {
                self.rows.iter().position(|row| {
                    row.track_id == Some(id)
                        && (self.page != StatisticsPage::Recent || row.date == date)
                })
            })
            .unwrap_or(self.selected)
            .min(self.rows.len().saturating_sub(1));
    }
}
struct WriteJob {
    receiver: Background<()>,
    records: Vec<ListeningRecord>,
}
#[derive(Default)]
pub struct StatisticsState {
    pub view: StatisticsView,
    tracker: ListeningTracker,
    pending: HashMap<String, ListeningRecord>,
    writing: Option<WriteJob>,
    reading: Option<Background<ListeningStatistics>>,
    read_requested: bool,
    last_checkpoint: Option<Instant>,
    last_listened_ms: u64,
    retry_at: Option<Instant>,
}
impl StatisticsState {
    fn enqueue(&mut self, record: ListeningRecord) {
        let entry = self
            .pending
            .entry(record.session_id.clone())
            .or_insert_with(|| record.clone());
        entry.listened_ms = entry.listened_ms.max(record.listened_ms);
        entry.qualified |= record.qualified;
        entry.updated_at_ms = entry.updated_at_ms.max(record.updated_at_ms);
        entry.ended_at_ms = record.ended_at_ms.or(entry.ended_at_ms);
    }
}
impl App {
    fn observe_listening(&mut self) -> bool {
        self.playback
            .audio
            .as_ref()
            .is_some_and(|audio| self.statistics.tracker.observe(audio.listened_time()))
    }
    fn checkpoint_listening(&mut self) {
        self.observe_listening();
        if let Some(record) = self.statistics.tracker.checkpoint() {
            self.statistics.last_listened_ms = record.listened_ms;
            self.statistics.enqueue(record);
            self.statistics.last_checkpoint = Some(Instant::now());
        }
    }
    pub(super) fn begin_listening(&mut self, track_id: i64, duration_ms: u64) {
        self.statistics.tracker.begin(track_id, duration_ms);
        self.statistics.last_checkpoint = None;
        self.statistics.last_listened_ms = 0;
    }
    pub(super) fn finish_listening(&mut self) {
        self.observe_listening();
        if let Some(record) = self.statistics.tracker.finish() {
            self.statistics.enqueue(record);
        }
    }
    pub(super) fn open_statistics(&mut self) {
        self.cancel_workspace_drag();
        self.checkpoint_listening();
        self.statistics.read_requested = true;
        self.statistics.view.loading = true;
        self.statistics.retry_at = None;
        self.statistics.view.focused = true;
        self.view.dialog = Some(Dialog::Statistics);
    }

    pub(super) fn statistics_background_input(&mut self, input: &Input) -> bool {
        if !matches!(self.view.dialog, Some(Dialog::Statistics)) {
            return false;
        }
        match input {
            Input::Click { x, y, .. } | Input::SecondaryClick { x, y } => {
                self.statistics.view.focused =
                    self.view.overlay_area.contains(Position::new(*x, *y));
                if self.statistics.view.focused {
                    self.cancel_workspace_drag();
                }
                !self.statistics.view.focused
            }
            Input::Scroll { x, y, .. } => {
                self.statistics.view.focused =
                    self.view.overlay_area.contains(Position::new(*x, *y));
                !self.statistics.view.focused
            }
            Input::Key(_)
            | Input::Text(_)
            | Input::Drag { .. }
            | Input::Release { .. }
            | Input::CancelPointer => !self.statistics.view.focused,
            _ => false,
        }
    }

    pub(super) fn open_statistics_track(&mut self) -> Result<()> {
        let Some(track_id) = self
            .statistics
            .view
            .rows
            .get(self.statistics.view.selected)
            .and_then(|row| row.track_id)
        else {
            return Ok(());
        };
        anyhow::ensure!(!self.busy(), crate::errors::AppError::LibraryBusy);
        let destination = self
            .library
            .playlists
            .iter()
            .filter_map(|playlist| {
                playlist
                    .entries
                    .iter()
                    .find(|entry| entry.track.id == track_id)
                    .map(|entry| (playlist, entry.id))
            })
            .min_by_key(|(playlist, _)| {
                (
                    Some(playlist.id) != self.library.selected_playlist,
                    playlist.mode != self.settings.mode,
                    playlist.kind == PlaylistKind::SortingDesk,
                )
            })
            .map(|(playlist, entry)| {
                (
                    playlist.id,
                    entry,
                    if playlist.kind == PlaylistKind::SortingDesk {
                        self.settings.mode
                    } else {
                        playlist.mode
                    },
                )
            });
        let Some((playlist, entry, mode)) = destination else {
            self.message(
                self.text(
                    "This track is no longer in any playlist.",
                    "Этот трек больше не входит ни в один плейлист.",
                )
                .into(),
            );
            return Ok(());
        };
        if self.settings.mode != mode {
            self.set_mode(mode)?;
        }
        let workspace = &mut self.settings.preferences.workspace;
        let hidden = workspace.hidden.contains(&Panel::Tracks)
            || workspace.collapsed.contains(&Panel::Tracks);
        workspace.hidden.retain(|panel| *panel != Panel::Tracks);
        workspace.collapsed.retain(|panel| *panel != Panel::Tracks);
        if hidden {
            self.save_settings()?;
        }
        self.library.query.clear();
        self.view.filter_editing = false;
        self.view.filter_keyboard = false;
        self.select_playlist(playlist);
        self.library.selected_entry = Some(entry);
        self.view.playlist_scroll_selected = None;
        self.focus_panel(Panel::Tracks);
        self.refresh()?;
        self.close_dialog();
        Ok(())
    }
    pub(super) fn tick_statistics(&mut self) -> Result<()> {
        let open = matches!(self.view.dialog, Some(Dialog::Statistics));
        let interval = Duration::from_secs(if open { 1 } else { 5 });
        let qualified = self.observe_listening();
        if qualified
            || self
                .statistics
                .last_checkpoint
                .is_none_or(|time| time.elapsed() >= interval)
        {
            // Pauses do not generate identical periodic database writes.
            if let Some(record) = self.statistics.tracker.checkpoint()
                && record.listened_ms != self.statistics.last_listened_ms
            {
                self.statistics.last_listened_ms = record.listened_ms;
                self.statistics.enqueue(record);
                self.statistics.last_checkpoint = Some(Instant::now());
            }
        }
        if let Some(result) = self
            .statistics
            .writing
            .as_mut()
            .and_then(|job| poll(&mut job.receiver))
        {
            let job = self.statistics.writing.take().unwrap();
            match result {
                Ok(()) => {
                    self.statistics.retry_at = None;
                    self.statistics.view.error = None;
                    self.statistics.read_requested |= open;
                }
                Err(error) => {
                    for record in job.records {
                        self.statistics.enqueue(record);
                    }
                    self.statistics.retry_at = Some(Instant::now() + Duration::from_secs(5));
                    self.statistics.view.error = Some(crate::errors::message(
                        &error,
                        self.settings.preferences.language,
                    ));
                    self.failure(error);
                }
            }
        }
        if let Some(result) = self.statistics.reading.as_mut().and_then(poll) {
            self.statistics.reading = None;
            self.statistics.view.loading = false;
            match result {
                Ok(snapshot) => {
                    self.statistics.view.snapshot = snapshot;
                    self.statistics.view.rebuild();
                    self.statistics.view.error = None;
                }
                Err(error) => {
                    self.statistics.view.error = Some(crate::errors::message(
                        &error,
                        self.settings.preferences.language,
                    ))
                }
            }
        }
        if self.statistics.writing.is_none()
            && !self.statistics.pending.is_empty()
            && self
                .statistics
                .retry_at
                .is_none_or(|time| Instant::now() >= time)
        {
            let mut store = self.store.try_clone()?;
            let records = self
                .statistics
                .pending
                .drain()
                .map(|(_, record)| record)
                .collect::<Vec<_>>();
            let saving = records.clone();
            let receiver = background(self.runtime.as_ref().unwrap(), move || {
                store.record_listening(&saving)
            });
            self.statistics.writing = Some(WriteJob { receiver, records });
        }
        if open
            && self.statistics.read_requested
            && self.statistics.reading.is_none()
            && self.statistics.writing.is_none()
            && self.statistics.pending.is_empty()
        {
            let store = self.store.try_clone()?;
            self.statistics.reading = Some(background(self.runtime.as_ref().unwrap(), move || {
                store.listening_statistics()
            }));
            self.statistics.read_requested = false;
        }
        Ok(())
    }
    pub(super) fn statistics_target(&mut self, target: Target) {
        match target {
            Target::StatisticsPage(page) => {
                self.statistics.view.page = page;
                self.statistics.view.rows.clear();
                self.statistics.view.selected = 0;
                self.statistics.view.offset = 0;
                self.statistics.view.rebuild();
            }
            Target::StatisticsSort => {
                self.statistics.view.sort = match self.statistics.view.sort {
                    StatisticsSort::Time => StatisticsSort::Plays,
                    StatisticsSort::Plays => StatisticsSort::Starts,
                    StatisticsSort::Starts => StatisticsSort::Name,
                    StatisticsSort::Name => StatisticsSort::Time,
                };
                self.statistics.view.rebuild();
                self.statistics.view.offset = 0;
            }
            Target::StatisticsRow(row) => {
                self.statistics.view.selected =
                    row.min(self.statistics.view.rows.len().saturating_sub(1))
            }
            Target::StatisticsRefresh => self.open_statistics(),
            _ => {}
        }
    }
    pub(super) fn move_statistics(&mut self, direction: i64) {
        self.statistics.view.selected = bounded(
            self.statistics.view.selected,
            direction,
            self.statistics.view.rows.len(),
        );
    }
    pub(super) fn statistics_key(&mut self, key: KeyPress) -> Result<()> {
        match key.key {
            Key::Escape => {
                self.close_dialog();
            }
            Key::Enter if key == KeyPress::plain(Key::Enter) => self.open_statistics_track()?,
            Key::Left | Key::Right | Key::Tab => {
                let index = StatisticsPage::ALL
                    .iter()
                    .position(|page| *page == self.statistics.view.page)
                    .unwrap();
                let direction = if key.key == Key::Left || (key.key == Key::Tab && key.shift) {
                    -1
                } else {
                    1
                };
                let next = (index as i64 + direction).rem_euclid(4) as usize;
                self.statistics_target(Target::StatisticsPage(StatisticsPage::ALL[next]));
            }
            Key::Up => self.move_statistics(-1),
            Key::Down => self.move_statistics(1),
            Key::PageUp => self.move_statistics(-(self.statistics.view.capacity.max(1) as i64)),
            Key::PageDown => self.move_statistics(self.statistics.view.capacity.max(1) as i64),
            Key::Home => self.statistics.view.selected = 0,
            Key::End => {
                self.statistics.view.selected = self.statistics.view.rows.len().saturating_sub(1)
            }
            Key::Char('s' | 'S') => self.statistics_target(Target::StatisticsSort),
            Key::Char('r' | 'R') => self.open_statistics(),
            _ => {
                if let Some(action) = self
                    .settings
                    .preferences
                    .bindings
                    .iter()
                    .find(|binding| binding.key == key)
                    .map(|binding| binding.action)
                    && matches!(
                        action,
                        Action::TogglePlay
                            | Action::Stop
                            | Action::Next
                            | Action::Previous
                            | Action::VolumeUp
                            | Action::VolumeDown
                            | Action::SeekForward
                            | Action::SeekBackward
                            | Action::Quit
                    )
                {
                    self.action(action)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn shutdown_statistics(
        &mut self,
        runtime: &Runtime,
        deadline: Instant,
    ) -> Result<()> {
        if let Some(job) = self.statistics.writing.take() {
            let result = runtime
                .block_on(async {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), job.receiver)
                        .await
                })
                .context("Listening history save did not finish before shutdown")?
                .context("Listening history save was interrupted")?;
            if result.is_err() {
                for record in job.records {
                    self.statistics.enqueue(record);
                }
            }
        }
        if !self.statistics.pending.is_empty() {
            let records = self
                .statistics
                .pending
                .drain()
                .map(|(_, record)| record)
                .collect::<Vec<_>>();
            let mut store = self.store.try_clone()?;
            let job = background(runtime, move || store.record_listening(&records));
            runtime
                .block_on(async {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), job).await
                })
                .context("Listening history save did not finish before shutdown")?
                .context("Listening history save was interrupted")??;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::ImportedTrack, store::Store};

    fn app(root: &std::path::Path) -> (App, i64) {
        let mut app = App::new(root).unwrap();
        let playlist = app.store.playlists().unwrap()[0].id;
        app.store
            .add_tracks(
                playlist,
                &[ImportedTrack {
                    path: root.join("music.wav"),
                    title: "Music".into(),
                    artist: "Artist".into(),
                    album: "Album".into(),
                    duration_ms: 180_000,
                    tags: "{}".into(),
                }],
                true,
            )
            .unwrap();
        let track = app.store.playlists().unwrap()[0].entries[0].track.id;
        (app, track)
    }
    fn wait(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.statistics.writing.is_some()
            || !app.statistics.pending.is_empty()
            || app.statistics.view.loading
        {
            assert!(Instant::now() < deadline, "{}", app.view.notice);
            app.tick_statistics().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn shutdown_flushes_the_last_partial_session_without_needing_another_tick() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, track) = app(directory.path());
        app.begin_listening(track, 180_000);
        app.statistics.tracker.observe(Duration::from_secs(17));
        drop(app);
        let saved = Store::open(directory.path())
            .unwrap()
            .listening_statistics()
            .unwrap();
        assert_eq!(saved.summary.listened_ms, 17_000);
        assert_eq!(saved.summary.plays, 0);
        assert_eq!(saved.summary.starts, 1);
    }
    #[test]
    fn final_checkpoints_coalesce_and_a_repeated_track_starts_a_new_session() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, track) = app(directory.path());
        app.begin_listening(track, 180_000);
        app.statistics.tracker.observe(Duration::from_secs(90));
        app.checkpoint_listening();
        app.tick_statistics().unwrap(); // Begin an asynchronous save.
        app.statistics.tracker.observe(Duration::from_secs(120));
        app.finish_listening();
        app.begin_listening(track, 180_000);
        app.statistics.tracker.observe(Duration::from_secs(90));
        app.finish_listening();
        app.open_statistics();
        wait(&mut app);
        let summary = &app.statistics.view.snapshot.summary;
        assert_eq!(summary.listened_ms, 210_000);
        assert_eq!(summary.plays, 2);
        assert_eq!(summary.starts, 2);
    }
    #[test]
    fn failed_background_write_retains_records_and_retry_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, track) = app(directory.path());
        let connection = rusqlite::Connection::open(&app.store.path).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON listening_sessions BEGIN SELECT RAISE(ABORT,'history unavailable'); END;").unwrap();
        app.begin_listening(track, 180_000);
        app.statistics.tracker.observe(Duration::from_secs(90));
        app.finish_listening();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.statistics.retry_at.is_none() {
            assert!(Instant::now() < deadline);
            app.tick_statistics().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(app.statistics.pending.len(), 1);
        assert!(app.statistics.view.error.is_some());
        connection
            .execute_batch("DROP TRIGGER fail_history")
            .unwrap();
        app.open_statistics();
        wait(&mut app);
        assert!(app.statistics.view.error.is_none());
        assert_eq!(app.statistics.view.snapshot.summary.plays, 1);
        assert_eq!(app.statistics.view.snapshot.summary.listened_ms, 90_000);
    }
}
