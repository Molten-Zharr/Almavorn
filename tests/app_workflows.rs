use almavorn::{
    app::{App, Browser, BrowserEntry, Dialog, Focus, Sort, Target},
    input::{Action, Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode, Placement, PlaylistKind, Track},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "almavorn-workflows-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn key(app: &mut App, key: Key) {
    app.handle(Input::Key(KeyPress::plain(key)));
}

fn control(app: &mut App, character: char) {
    app.handle(Input::Key(KeyPress {
        key: Key::Char(character),
        ctrl: true,
        alt: false,
        shift: false,
    }));
    wait_for_library(app);
}

fn submit_text(app: &mut App, text: &str) {
    assert!(app.accepts_text());
    app.handle(Input::Text(text.into()));
    key(app, Key::Enter);
    wait_for_library(app);
    assert!(!app.view.notice_error, "{}", app.view.notice);
    assert!(app.view.dialog.is_none());
}

fn create_playlist(app: &mut App, name: &str) -> i64 {
    app.action(Action::NewPlaylist).unwrap();
    submit_text(app, name);
    app.library.selected_playlist.unwrap()
}

fn click_action(app: &mut App, action: Action) {
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    let area = app
        .view
        .hits
        .iter()
        .find(|hit| hit.enabled && matches!(hit.target, Target::Action(value) if value == action))
        .unwrap()
        .area;
    app.handle(Input::Click {
        x: area.x,
        y: area.y,
        double: false,
    });
}

#[test]
fn keyboard_and_mouse_respect_order_editing_and_persist_settings() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    key(&mut app, Key::Char('c'));
    submit_text(&mut app, "Keyboard playlist");
    let first = app.library.selected_playlist.unwrap();
    click_action(&mut app, Action::NewPlaylist);
    submit_text(&mut app, "Mouse playlist");
    assert_ne!(app.library.selected_playlist.unwrap(), first);

    app.view.focus = Focus::Playlists;
    assert!(!app.allowed(Action::Rename));
    app.action(Action::Rename).unwrap();
    assert!(app.view.dialog.is_none());
    click_action(&mut app, Action::ToggleEdit);
    assert!(app.allowed(Action::Rename));
    key(&mut app, Key::F(3));
    submit_text(&mut app, "Renamed playlist");
    assert_eq!(app.playlist().unwrap().name, "Renamed playlist");

    // The Russian-layout key for M must use the same mode-switching action.
    key(&mut app, Key::Char('ь'));
    assert_eq!(app.settings.mode, Mode::Chaos);
    assert!(!app.view.editing);
    wait_for_settings(&mut app);
    assert_eq!(app.store.settings().unwrap().mode, Mode::Chaos);
    drop(app);
    let reopened = App::new(&directory.0).unwrap();
    assert_eq!(reopened.settings.mode, Mode::Chaos);
    assert!(!reopened.view.editing);
    assert!(
        reopened
            .playlists()
            .iter()
            .any(|p| p.name == "Renamed playlist")
    );
}

#[test]
fn chaos_undo_and_redo_leave_order_playlists_unchanged() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    create_playlist(&mut app, "Protected");
    let order = app.store.snapshot("order").unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    let id = create_playlist(&mut app, "Original");
    app.view.focus = Focus::Playlists;
    app.action(Action::Rename).unwrap();
    submit_text(&mut app, "Renamed");

    control(&mut app, 'z');
    assert_eq!(app.playlist().unwrap().name, "Original");
    control(&mut app, 'z');
    assert!(app.visible_playlists().is_empty());
    control(&mut app, 'y');
    assert_eq!(app.playlist().unwrap().id, id);
    control(&mut app, 'y');
    assert_eq!(app.playlist().unwrap().name, "Renamed");
    assert_eq!(app.store.snapshot("order").unwrap(), order);
}

#[test]
fn filtering_and_sorting_do_not_modify_saved_track_order() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    let id = create_playlist(&mut app, "Music");
    let tracks = [
        ("Zebra", "Alice", "First"),
        ("Alpha", "Bob", "Second"),
        ("Middle", "Alice", "Second"),
    ]
    .map(|(title, artist, album)| ImportedTrack {
        path: directory.0.join(format!("{title}.wav")),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration_ms: 1_000,
        tags: "{}".into(),
    });
    app.store.add_tracks(id, &tracks, false).unwrap();
    wait_for_library(&mut app);
    app.set_mode(Mode::Chaos).unwrap();
    let before = app.store.snapshot("chaos").unwrap();
    app.action(Action::Sort).unwrap();
    assert!(app.library.sort == Sort::Title);
    assert_eq!(
        app.rows()
            .iter()
            .map(|e| e.track.title.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Middle", "Zebra"]
    );
    app.action(Action::Search).unwrap();
    submit_text(&mut app, "ALICE second");
    assert_eq!(app.rows().len(), 1);
    assert_eq!(app.entry().unwrap().track.title, "Middle");
    assert!(!app.allowed(Action::MoveUp));
    assert_eq!(app.store.snapshot("chaos").unwrap(), before);
    key(&mut app, Key::Escape);
    assert!(app.library.query.is_empty());
    assert_eq!(app.rows().len(), 3);
}

#[test]
fn cached_rows_follow_view_changes_and_background_reloads() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    let first = create_playlist(&mut app, "First");
    let track = |name: &str, file: &str, duration_ms| ImportedTrack {
        path: directory.0.join(file),
        title: name.into(),
        artist: "Артист".into(),
        album: "Album".into(),
        duration_ms,
        tags: String::new(),
    };
    app.store
        .add_tracks(
            first,
            &[
                track("Яблоко", "a.wav", 200),
                track("Beta", "b.wav", 100),
                track("Beta", "c.wav", 300),
            ],
            false,
        )
        .unwrap();
    wait_for_library(&mut app);
    let ids = || app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>();
    assert_eq!(ids(), ids());
    let original = app.store.snapshot("chaos").unwrap();
    app.library.query = "АРТИСТ beta".into();
    app.library.sort = Sort::Duration;
    assert_eq!(
        app.rows()
            .iter()
            .map(|entry| entry.track.duration_ms)
            .collect::<Vec<_>>(),
        [100, 300]
    );
    app.library.sort_descending = true;
    assert_eq!(
        app.rows()
            .iter()
            .map(|entry| entry.track.duration_ms)
            .collect::<Vec<_>>(),
        [300, 100]
    );
    app.library.sort = Sort::Title;
    assert_eq!(
        app.rows()
            .iter()
            .map(|entry| entry.position)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    app.library.query = "ЯБЛОКО".into();
    let selected = app.rows()[0].track.id;
    app.store
        .rename_track(selected, "New title", first, false)
        .unwrap();
    wait_for_library(&mut app);
    assert!(app.rows().is_empty());
    app.library.query = "new".into();
    assert_eq!(app.rows()[0].track.title, "New title");
    // Track title edits do not change playlist-reference snapshots.
    assert_eq!(app.store.snapshot("chaos").unwrap(), original);
    app.library.query.clear();
    let second = create_playlist(&mut app, "Second");
    assert!(app.rows().is_empty());
    app.store
        .add_tracks(second, &[track("Other", "d.wav", 400)], false)
        .unwrap();
    wait_for_library(&mut app);
    assert_eq!(app.rows().len(), 1);
    app.library.selected_playlist = Some(first);
    assert_eq!(app.rows().len(), 3);
    app.library.query = "missing".into();
    assert!(app.rows().is_empty());
}

fn write_wave(path: &Path) -> Vec<u8> {
    let samples = vec![128_u8; 8_000];
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + samples.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(8_000_u32.to_le_bytes());
    bytes.extend(8_000_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(8_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples.len() as u32).to_le_bytes());
    bytes.extend(samples);
    fs::write(path, &bytes).unwrap();
    bytes
}

fn wait_for_library(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.busy() {
        assert!(
            Instant::now() < deadline,
            "Library operation did not finish: {}",
            app.view.notice
        );
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_settings(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let expected = serde_json::to_value(&app.settings).unwrap();
    while serde_json::to_value(app.store.settings().unwrap()).unwrap() != expected {
        assert!(
            Instant::now() < deadline,
            "Settings were not saved: {}",
            app.view.notice
        );
        app.tick();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn background_import_routes_to_desk_skips_duplicates_and_supports_undo() {
    let directory = TestDirectory::new();
    let music = directory.0.join("music");
    fs::create_dir(&music).unwrap();
    let path = music.join("example.wav");
    let original = write_wave(&path);
    let invalid = directory.0.join("invalid.wav");
    fs::write(&invalid, b"invalid audio").unwrap();
    let mut app = App::new(&directory.0).unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    let before = app.store.snapshot("desk").unwrap();
    app.start_import(vec![music, path.clone(), invalid.clone()])
        .unwrap();
    assert!(app.busy());
    assert!(app.importing());
    assert!(!app.allowed(Action::AddFiles));
    app.set_mode(Mode::Order).unwrap();
    assert_eq!(app.settings.mode, Mode::Chaos);
    wait_for_library(&mut app);

    assert!(app.view.notice_error);
    assert_eq!(app.settings.mode, Mode::Order);
    assert_eq!(app.playlist().unwrap().kind, PlaylistKind::SortingDesk);
    assert_eq!(app.rows().len(), 1);
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&invalid).unwrap(), b"invalid audio");
    let imported = app.store.snapshot("desk").unwrap();
    control(&mut app, 'z');
    assert_eq!(app.store.snapshot("desk").unwrap(), before);
    control(&mut app, 'y');
    assert_eq!(app.store.snapshot("desk").unwrap(), imported);
    app.start_import(vec![path]).unwrap();
    wait_for_library(&mut app);
    assert!(!app.view.notice_error, "{}", app.view.notice);
    assert_eq!(app.rows().len(), 1);
}

#[test]
fn shortcut_capture_rejects_reserved_and_duplicate_keys() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    let index = app
        .settings
        .bindings
        .iter()
        .position(|b| b.action == Action::Quit)
        .unwrap();
    let original = app.settings.bindings[index].key;
    app.view.dialog = Some(Dialog::CaptureBinding { index });
    key(&mut app, Key::Down);
    assert!(app.view.notice_error);
    assert_eq!(app.settings.bindings[index].key, original);
    key(&mut app, Key::Char(' '));
    assert!(app.view.notice_error);
    assert_eq!(app.settings.bindings[index].key, original);
    control(&mut app, 'j');
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
    wait_for_settings(&mut app);
    assert_eq!(
        app.store.settings().unwrap().bindings[index].key,
        app.settings.bindings[index].key
    );
    key(&mut app, Key::Escape);
    control(&mut app, 'j');
    assert!(app.view.quit);
}

#[test]
fn dialogs_capture_clicks_and_render_across_languages_and_panel_positions() {
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        for placement in [
            Placement::Left,
            Placement::Right,
            Placement::Top,
            Placement::Bottom,
        ] {
            app.settings.playlist_placement = placement;
            app.settings.player_placement = placement;
            for (width, height) in [(120, 50), (70, 30), (32, 18), (20, 8)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
            }
        }
        for action in [Action::Help, Action::Settings, Action::NewPlaylist] {
            app.action(action).unwrap();
            let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
            terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
            assert!(app.view.dialog.is_some());
            assert!(!app.view.hits.iter().any(|hit| matches!(
                hit.target,
                Target::Playlist(_) | Target::Track(_) | Target::Mode(_)
            )));
            assert!(
                app.view
                    .hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::CloseDialog))
            );
            key(&mut app, Key::Escape);
        }
        let playlist = app.library.selected_playlist.unwrap();
        let track = Track {
            id: 1,
            path: directory.0.join("example.wav"),
            title: "Example".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_ms: 1_000,
            tags: "{}".into(),
        };
        for dialog in [
            Dialog::Browser(Browser {
                directory: directory.0.clone(),
                entries: vec![BrowserEntry {
                    path: track.path.clone(),
                    directory: false,
                }],
                selected: 0,
                offset: 0,
                marked: HashSet::new(),
                folder: false,
            }),
            Dialog::RemoveEntry {
                playlist,
                entry: 1,
                title: track.title.clone(),
            },
            Dialog::Transfer {
                ids: vec![track.id],
                selected: 0,
            },
            Dialog::Metadata { track, offset: 0 },
            Dialog::CaptureBinding { index: 0 },
        ] {
            app.view.dialog = Some(dialog);
            let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
            terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
            assert!(
                app.view
                    .hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::CloseDialog))
            );
            key(&mut app, Key::Escape);
        }
    }
}

#[test]
fn rendering_uses_the_palette_selected_for_each_mode() {
    use ratatui::style::Color;
    let directory = TestDirectory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.appearance.palette_ids = ["classic-amber".into(), "classic-violet".into()];
    for (mode, accent) in [
        (Mode::Order, Color::Rgb(232, 158, 74)),
        (Mode::Chaos, Color::Rgb(188, 125, 228)),
    ] {
        app.set_mode(mode).unwrap();
        for (language, width, height) in [
            (Language::Russian, 80, 30),
            (Language::English, 80, 30),
            (Language::Russian, 120, 50),
            (Language::English, 120, 50),
        ] {
            app.settings.language = language;
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
            let buffer = terminal.backend().buffer();
            assert!(
                buffer.content.iter().any(|cell| cell.fg == accent),
                "Active mode must use its own palette"
            );
            assert_eq!(buffer[(0, 0)].bg, Color::Rgb(18, 22, 29));
            assert!(
                app.view.hits.iter().any(
                    |hit| hit.enabled && matches!(hit.target, Target::Action(Action::Settings))
                ),
                "{}",
                buffer
                    .content
                    .iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
            );
            assert!(app.view.tracks_area.width > 0 && app.view.tracks_area.height > 0);
            assert!(app.view.playlist_area.width > 0 && app.view.playlist_area.height > 0);
        }
    }
}
