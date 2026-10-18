use super::*;
use crate::{
    app::Sort,
    model::{Mode, PlaylistKind, Track},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

const MAIN: i64 = 1;
const OTHER: i64 = 2;
const ORDER: i64 = 3;

fn fixture() -> (tempfile::TempDir, App) {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    let track = |id, title: &str, artist: &str, album: &str, path: &str| Track {
        id,
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        path: PathBuf::from(path),
        duration_ms: 1000,
        tags: "{\"note\":\"tagonly\"}".into(),
    };
    let tracks = [
        track(
            101,
            "Ночная волна",
            "Ёлка & Guest",
            "City Live",
            "/MUSIC/Concerts/NEON.WAV",
        ),
        track(
            102,
            "City Lights",
            "ELKA",
            "Ночная Sessions",
            "/MUSIC/Studio/calm.flac",
        ),
        track(
            103,
            "CAFÉ İSTANBUL",
            "Björk",
            "Straße",
            "/MUSIC/archive/WAVE.wav",
        ),
        track(104, "Noise", "Nobody", "Else", "/MUSIC/last/night.ogg"),
        track(105, "Wave", "Another", "City", "/MUSIC/Live/other.wav"),
    ];
    let playlist = |id, mode, entries| Playlist {
        id,
        name: format!("Playlist {id}"),
        mode,
        kind: PlaylistKind::Normal,
        position: id,
        entries,
        folders: Vec::new(),
        group_folders: false,
    };
    app.library.playlists = vec![
        playlist(
            MAIN,
            Mode::Chaos,
            tracks
                .iter()
                .enumerate()
                .map(|(position, track)| Entry {
                    id: 11 + position as i64,
                    position: position as i64,
                    track: track.clone(),
                })
                .collect(),
        ),
        playlist(
            OTHER,
            Mode::Chaos,
            vec![Entry {
                id: 21,
                position: 0,
                track: tracks[0].clone(),
            }],
        ),
        playlist(
            ORDER,
            Mode::Order,
            vec![Entry {
                id: 31,
                position: 0,
                track: tracks[0].clone(),
            }],
        ),
    ];
    app.library.selected_playlist = Some(MAIN);
    (directory, app)
}

fn search(app: &mut App) -> &mut SearchDialog {
    let Some(Dialog::Search(search)) = &mut app.view.dialog else {
        panic!("Search must be open")
    };
    search
}

fn finish_search(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        app.tick_search().unwrap();
        let search = search(app);
        assert!(!search.failed);
        if !search.loading {
            break;
        }
        assert!(Instant::now() < deadline, "Search did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn hits(app: &App) -> Vec<(i64, i64)> {
    let Some(Dialog::Search(search)) = &app.view.dialog else {
        panic!("Search must be open")
    };
    search
        .results
        .iter()
        .map(|hit| {
            let (playlist, entry) = app.search_entry(*hit).unwrap();
            (playlist.id, entry.id)
        })
        .collect()
}

#[test]
fn local_filter_and_global_search_preserve_the_same_literal_unicode_and_all_word_rules() {
    let (_directory, mut app) = fixture();
    app.open_search();
    search(&mut app).playlists = HashSet::from([MAIN]);
    let cases: &[(&str, &[i64])] = &[
        ("", &[11, 12, 13, 14, 15]),
        (" \t\n\u{2003} ", &[11, 12, 13, 14, 15]),
        ("НОЧНАЯ\u{2003} CITY", &[11, 12]),
        ("city ночная city", &[11, 12]),
        ("ЁЛКА live", &[11]),
        ("елка", &[]),
        ("ELKA", &[12]),
        ("NEON.WAV", &[11]),
        ("city live", &[11, 15]),
        ("city \"live\"", &[]),
        ("city*", &[]),
        ("-Guest", &[]),
        ("CafÉ", &[13]),
        ("Cafe", &[]),
        ("CAFE\u{0301}", &[]),
        ("STRAßE", &[13]),
        ("STRASSE", &[]),
        ("İSTANBUL", &[13]),
        ("i\u{0307}stan", &[13]),
        ("night", &[14]),
        ("tagonly", &[]),
        ("1000", &[]),
        ("Playlist", &[]),
    ];
    for (query, expected) in cases {
        app.library.query = (*query).into();
        assert_eq!(
            app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>(),
            *expected,
            "Local query {query:?}"
        );
        search(&mut app).query = (*query).into();
        finish_search(&mut app);
        assert_eq!(
            hits(&app),
            expected.iter().map(|id| (MAIN, *id)).collect::<Vec<_>>(),
            "Global query {query:?}"
        );
    }
}

#[test]
fn global_search_keeps_each_playlist_entry_and_its_order_independent_of_local_sorting() {
    let (_directory, mut app) = fixture();
    app.library.sort = Sort::Title;
    app.library.query = "city".into();
    assert_eq!(
        app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [12, 15, 11]
    );
    app.open_search();
    search(&mut app).query = "city".into();
    finish_search(&mut app);
    assert_eq!(
        hits(&app),
        [(MAIN, 11), (MAIN, 12), (MAIN, 15), (OTHER, 21), (ORDER, 31)]
    );
    search(&mut app).playlists = HashSet::from([OTHER, ORDER]);
    finish_search(&mut app);
    assert_eq!(hits(&app), [(OTHER, 21), (ORDER, 31)]);
    search(&mut app).playlists.clear();
    finish_search(&mut app);
    assert!(hits(&app).is_empty());
    assert_eq!(app.library.query, "city");
    assert_eq!(
        app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [12, 15, 11]
    );
}

#[test]
fn metadata_revision_rebuilds_both_the_local_rows_and_the_global_search_index() {
    let (_directory, mut app) = fixture();
    app.library.query = "ночная city".into();
    app.open_search();
    search(&mut app).playlists = HashSet::from([MAIN]);
    search(&mut app).query = app.library.query.clone();
    finish_search(&mut app);
    assert_eq!(app.rows().len(), 2);
    assert_eq!(hits(&app), [(MAIN, 11), (MAIN, 12)]);
    for playlist in &mut app.library.playlists {
        for entry in &mut playlist.entries {
            if entry.track.id == 101 {
                entry.track.title = "Sunrise".into();
                entry.track.album = "Studio".into();
            }
        }
    }
    app.library.revision += 1;
    finish_search(&mut app);
    assert_eq!(
        app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [12]
    );
    assert_eq!(hits(&app), [(MAIN, 12)]);
    app.library.query = "Sunrise studio".into();
    search(&mut app).query = app.library.query.clone();
    finish_search(&mut app);
    assert_eq!(
        app.rows().iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [11]
    );
    assert_eq!(hits(&app), [(MAIN, 11)]);
}

fn blocked_search(app: &mut App) -> oneshot::Sender<anyhow::Result<Vec<SearchHit>>> {
    let search = search(app);
    let request = SearchRequest {
        index: search.index.clone(),
        query: search.query.clone(),
        playlists: search.playlists.clone(),
    };
    search.loading = true;
    search.results.clear();
    let (sender, receiver) = oneshot::channel();
    app.search_job.request(request);
    app.search_job.start(|_| receiver);
    sender
}

#[test]
fn stale_background_results_cannot_replace_a_new_query_or_playlist_scope() {
    for scope_change in [false, true] {
        let (_directory, mut app) = fixture();
        app.open_search();
        search(&mut app).query = "city".into();
        search(&mut app).playlists = HashSet::from([MAIN]);
        finish_search(&mut app);
        let sender = blocked_search(&mut app);
        if scope_change {
            search(&mut app).playlists = HashSet::from([OTHER]);
        } else {
            search(&mut app).query = "night".into();
        }
        app.tick_search().unwrap();
        sender
            .send(Ok(vec![SearchHit {
                playlist: 0,
                entry: 0,
            }]))
            .unwrap();
        app.tick_search().unwrap();
        assert!(search(&mut app).loading);
        assert!(hits(&app).is_empty());
        finish_search(&mut app);
        assert_eq!(
            hits(&app),
            if scope_change {
                vec![(OTHER, 21)]
            } else {
                vec![(MAIN, 14)]
            }
        );
    }
}

#[test]
fn closing_and_reopening_search_discards_the_previous_dialogs_background_results() {
    let (_directory, mut app) = fixture();
    app.open_search();
    search(&mut app).query = "city".into();
    finish_search(&mut app);
    let sender = blocked_search(&mut app);
    app.view.dialog = None;
    app.tick_search().unwrap();
    sender
        .send(Ok(vec![SearchHit {
            playlist: 0,
            entry: 0,
        }]))
        .unwrap();
    app.tick_search().unwrap();
    assert!(app.view.dialog.is_none());
    app.open_search();
    search(&mut app).query = "night".into();
    finish_search(&mut app);
    assert_eq!(hits(&app), [(MAIN, 14)]);
}

#[test]
fn query_editing_has_the_same_unicode_limit_control_sanitization_and_selection_rules() {
    let (_directory, mut app) = fixture();
    for filter in [false, true] {
        if filter {
            app.view.dialog = None;
            app.library.query = "before".into();
            app.edit_filter().unwrap();
        } else {
            app.open_search();
            search(&mut app).query = "before".into();
            search(&mut app).selected_all = true;
        }
        app.query_text(&("é\n\t".repeat(600))).unwrap();
        let current = |app: &App| {
            if filter {
                app.library.query.clone()
            } else {
                let Some(Dialog::Search(search)) = &app.view.dialog else {
                    panic!("Search must be open")
                };
                search.query.clone()
            }
        };
        assert_eq!(current(&app), "é".repeat(512));
        if filter {
            app.filter_key(KeyPress::plain(Key::Backspace)).unwrap();
        } else {
            app.search_key(KeyPress::plain(Key::Backspace)).unwrap();
        }
        assert_eq!(current(&app), "é".repeat(511));
        if filter {
            app.view.filter_selected_all = true;
        } else {
            search(&mut app).selected_all = true;
        }
        app.query_text("\n\t").unwrap();
        assert!(current(&app).is_empty());
        app.query_text("Ночь").unwrap();
        assert_eq!(current(&app), "Ночь");
        if filter {
            app.filter_key(KeyPress::plain(Key::Escape)).unwrap();
            assert_eq!(app.library.query, "before");
        }
    }
    app.view.dialog = Some(Dialog::Help { offset: 0 });
    assert!(!app.query_text("ignored").unwrap());
}
