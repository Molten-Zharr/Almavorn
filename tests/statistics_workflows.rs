use almavorn::{
    app::{App, CommandMenu, Dialog, StatisticsPage, StatisticsSort, Target},
    input::{Action, Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode},
    statistics::ListeningRecord,
    store::Store,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, Instant};

fn seed(store: &mut Store, root: &std::path::Path) -> Vec<i64> {
    let playlist = store.playlists().unwrap()[0].id;
    let tracks = [
        ("Короткий", "Артист", "Альбом"),
        ("Длинный", "АРТИСТ", "АЛЬБОМ"),
        ("Другой", "Другой артист", "Альбом"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, artist, album))| ImportedTrack {
        path: root.join(format!("{index}.wav")),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration_ms: 180_000,
        tags: "{}".into(),
    })
    .collect::<Vec<_>>();
    store.add_tracks(playlist, &tracks, true).unwrap();
    store.playlists().unwrap()[0]
        .entries
        .iter()
        .map(|entry| entry.track.id)
        .collect()
}
fn record(id: &str, track_id: i64, heard: u64, qualified: bool, start: i64) -> ListeningRecord {
    ListeningRecord {
        session_id: id.into(),
        track_id,
        duration_ms: 180_000,
        started_at_ms: start,
        updated_at_ms: start + heard as i64,
        listened_ms: heard,
        qualified,
        ended_at_ms: None,
    }
}

#[test]
fn checkpoints_are_idempotent_and_unicode_metadata_is_grouped_without_merging_other_artists() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let ids = seed(&mut store, directory.path());
    let revision = store.revision();
    let partial = record("first", ids[0], 10_000, false, 1_700_000_000_000);
    store
        .record_listening(std::slice::from_ref(&partial))
        .unwrap();
    let mut qualified = partial.clone();
    qualified.listened_ms = 90_000;
    qualified.updated_at_ms += 80_000;
    qualified.qualified = true;
    qualified.ended_at_ms = Some(qualified.updated_at_ms);
    store
        .record_listening(&[qualified.clone(), qualified, partial])
        .unwrap();
    store
        .record_listening(&[
            record("second", ids[0], 5_000, false, 1_700_000_100_000),
            record("third", ids[1], 100_000, true, 1_700_000_200_000),
            record("fourth", ids[2], 20_000, false, 1_700_000_300_000),
            record("silent", ids[2], 0, false, 1_700_000_400_000),
        ])
        .unwrap();
    let statistics = store.listening_statistics().unwrap();
    assert_eq!(statistics.summary.listened_ms, 215_000);
    assert_eq!(statistics.summary.plays, 2);
    assert_eq!(statistics.summary.starts, 4);
    assert_eq!(statistics.summary.tracks, 3);
    assert_eq!(statistics.summary.artists, 2);
    assert_eq!(statistics.summary.albums, 2);
    assert_eq!(
        statistics
            .artists
            .iter()
            .find(|row| row.tracks == 2)
            .unwrap()
            .listened_ms,
        195_000
    );
    assert_eq!(statistics.recent[0].title, "Другой");
    assert_eq!(statistics.recent.len(), 4);
    assert_eq!(store.revision(), revision);
    drop(store);
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(
        reopened.listening_statistics().unwrap().summary.listened_ms,
        215_000
    );
}

#[test]
fn invalid_session_batch_rolls_back_and_retries_cannot_reassign_a_session() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let ids = seed(&mut store, directory.path());
    let session = record("same", ids[0], 25_000, false, 1_700_000_000_000);
    store
        .record_listening(std::slice::from_ref(&session))
        .unwrap();
    let mut collision = session;
    collision.track_id = ids[1];
    assert!(
        store
            .record_listening(&[
                record("new", ids[1], 90_000, true, 1_700_000_100_000),
                collision
            ])
            .is_err()
    );
    let history = store.listening_statistics().unwrap();
    assert_eq!(history.summary.starts, 1);
    assert_eq!(history.summary.listened_ms, 25_000);
}

#[test]
fn v4_library_upgrades_without_changing_music_or_settings_and_history_survives_undo() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let ids = seed(&mut store, directory.path());
    let mut settings = store.settings().unwrap();
    settings.preferences.language = Language::Russian;
    store.save_settings(&settings).unwrap();
    let before = store.snapshot("order").unwrap();
    let path = store.path.clone();
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("DROP TABLE listening_sessions; UPDATE almavorn_metadata SET value='4' WHERE key='schema_version'; PRAGMA user_version=4;").unwrap();
    drop(connection);
    let mut store = Store::open(directory.path()).unwrap();
    assert_eq!(before, store.snapshot("order").unwrap());
    assert_eq!(
        store.settings().unwrap().preferences.language,
        Language::Russian
    );
    assert_eq!(store.listening_statistics().unwrap().summary.starts, 0);
    store
        .record_listening(&[record("kept", ids[0], 90_000, true, 1_700_000_000_000)])
        .unwrap();
    let change = store.create_playlist("Undo", Mode::Chaos).unwrap();
    store
        .restore("chaos", &change.after, &change.before)
        .unwrap();
    assert_eq!(store.listening_statistics().unwrap().summary.plays, 1);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 5);
}

#[test]
fn recent_history_is_bounded_but_totals_include_every_session() {
    let directory = tempfile::tempdir().unwrap();
    let mut store = Store::open(directory.path()).unwrap();
    let ids = seed(&mut store, directory.path());
    let records = (0..125)
        .map(|index| {
            record(
                &format!("{index}"),
                ids[0],
                1000,
                false,
                1_700_000_000_000 + index * 2000,
            )
        })
        .collect::<Vec<_>>();
    store.record_listening(&records).unwrap();
    let history = store.listening_statistics().unwrap();
    assert_eq!(history.summary.starts, 125);
    assert_eq!(history.summary.listened_ms, 125_000);
    assert_eq!(history.recent.len(), 100);
    assert!(history.recent[0].last_at_ms > history.recent[99].last_at_ms);
}

fn render(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}
fn key(app: &mut App, key: Key) {
    app.handle(Input::Key(KeyPress::plain(key)));
}
fn wait(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.statistics.view.loading {
        assert!(Instant::now() < deadline, "{}", app.view.notice);
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn statistics_menu_shortcut_tabs_sorting_and_compact_mouse_controls_work_in_both_frontends() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    app.settings.preferences.language = Language::Russian;
    let ids = seed(&mut app.store, directory.path());
    app.store
        .record_listening(&[
            record("partial", ids[0], 20_000, false, 1_700_000_000_000),
            record("qualified", ids[1], 90_000, true, 1_700_000_100_000),
        ])
        .unwrap();
    assert!(
        app.command_items(CommandMenu::Application)
            .iter()
            .any(|item| matches!(item.target, Target::Action(Action::Statistics)) && item.enabled)
    );
    render(&mut app, 120, 40);
    let statistics = app
        .view
        .hits
        .iter()
        .find(|hit| hit.enabled && matches!(hit.target, Target::Action(Action::Statistics)))
        .unwrap()
        .area;
    app.handle(Input::Click {
        x: statistics.x,
        y: statistics.y,
        double: false,
    });
    assert!(matches!(app.view.dialog, Some(Dialog::Statistics)));
    wait(&mut app);
    assert_eq!(app.statistics.view.rows[0].title, "Длинный");
    let wide = render(&mut app, 120, 40);
    assert!(wide.contains("Время прослушивания"));
    key(&mut app, Key::Char('s'));
    assert_eq!(app.statistics.view.sort, StatisticsSort::Plays);
    key(&mut app, Key::Right);
    assert_eq!(app.statistics.view.page, StatisticsPage::Artists);
    assert_eq!(app.statistics.view.rows.len(), 1);
    key(&mut app, Key::Right);
    assert_eq!(app.statistics.view.page, StatisticsPage::Albums);
    key(&mut app, Key::Right);
    assert_eq!(app.statistics.view.page, StatisticsPage::Recent);
    assert_eq!(app.statistics.view.rows[0].title, "Длинный");
    let compact = render(&mut app, 32, 26);
    assert!(compact.contains("Длинный"));
    assert!(compact.contains("Зачёт: 50% или 4 мин."));
    assert!(compact.contains("Отрывки добавляют время."));
    assert!(compact.contains("Альб. 1"));
    key(&mut app, Key::Down);
    assert_eq!(app.statistics.view.selected, 1);
    key(&mut app, Key::Home);
    assert_eq!(app.statistics.view.selected, 0);
    render(&mut app, 32, 18);
    assert!(
        app.view
            .hits
            .iter()
            .any(|hit| matches!(hit.target, Target::CloseDialog))
    );
    render(&mut app, 32, 26);
    let close = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::CloseDialog))
        .unwrap()
        .area;
    app.handle(Input::Click {
        x: close.x,
        y: close.y,
        double: false,
    });
    assert!(app.view.dialog.is_none());
    key(&mut app, Key::F(7));
    wait(&mut app);
    key(&mut app, Key::F(7));
    assert!(app.view.dialog.is_none());
}
