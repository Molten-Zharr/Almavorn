use almavorn::{
    app::{App, CommandMenu, Dialog, Target},
    input::{Action, Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode, Track},
    store::Store,
    tuner::TunerSettings,
    ui,
};
use ratatui::{Terminal, backend::TestBackend, layout::Rect};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "almavorn-tuner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn key(app: &mut App, key: Key) {
    app.handle(Input::Key(KeyPress::plain(key)));
}
fn render(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    terminal
}
fn click(app: &mut App, predicate: impl Fn(&Target) -> bool) {
    render(app, 120, 50);
    let area = app
        .view
        .hits
        .iter()
        .find(|hit| hit.enabled && predicate(&hit.target))
        .unwrap()
        .area;
    app.handle(Input::Click {
        x: area.x,
        y: area.y,
        double: false,
    });
    app.handle(Input::Release {
        x: area.x,
        y: area.y,
    });
    assert!(!app.view.notice_error, "{}", app.view.notice);
}
fn saved(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let expected = serde_json::to_value(&app.settings).unwrap();
    while serde_json::to_value(app.store.settings().unwrap()).unwrap() != expected {
        assert!(Instant::now() < deadline, "{}", app.view.notice);
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn library_tracks(app: &mut App, directory: &Directory) -> Vec<Track> {
    let imported: Vec<_> = ["First", "Second"]
        .into_iter()
        .enumerate()
        .map(|(index, title)| ImportedTrack {
            path: directory.0.join(format!("{index}.wav")),
            title: title.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_ms: 1000,
            tags: "{}".into(),
        })
        .collect();
    let change = app.store.create_playlist("FX tracks", Mode::Chaos).unwrap();
    let playlist = change
        .after
        .playlists
        .iter()
        .find(|p| p.name == "FX tracks")
        .unwrap()
        .id;
    app.store.add_tracks(playlist, &imported, false).unwrap();
    app.store
        .playlists()
        .unwrap()
        .into_iter()
        .find(|p| p.id == playlist)
        .unwrap()
        .entries
        .into_iter()
        .map(|entry| entry.track)
        .collect()
}

#[test]
fn pinned_fx_edits_follow_the_playing_file_and_survive_profiles_playlists_and_restart() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let tracks = library_tracks(&mut app, &directory);
    app.settings.preferences.tuner.speed = 120;
    let global = app.settings.preferences.tuner.clone();
    let equalizer = app.settings.preferences.equalizer.clone();
    app.playback.current = Some(tracks[0].clone());
    // Library selection is independent from the track whose FX are being edited.
    app.library.selected_entry = Some(999);
    app.action(Action::PlaybackTuner).unwrap();
    click(&mut app, |target| matches!(target, Target::TunerPin));
    assert!(app.tuner_pinned());
    key(&mut app, Key::Right);
    key(&mut app, Key::Down);
    key(&mut app, Key::PageDown);
    let terminal = render(&mut app, 120, 50);
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("FX композиции: First"));
    let bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::TunerSlider(2, area) => Some(area),
            _ => None,
        })
        .unwrap();
    app.handle(Input::Scroll {
        x: bar.x,
        y: bar.y,
        delta: -1,
    });
    let bass_bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::TunerSlider(3, area) => Some(area),
            _ => None,
        })
        .unwrap();
    app.drag_tuner(3, bass_bar, 0.51, true);
    key(&mut app, Key::Char(' '));
    let pinned = TunerSettings {
        speed: 121,
        tempo: 90,
        pitch: 101,
        bass: 51,
        reverse: true,
    };
    assert_eq!(app.tuner_settings(), &pinned);
    assert_eq!(app.settings.preferences.tuner, global);
    assert_eq!(app.settings.preferences.equalizer, equalizer);
    saved(&mut app);
    assert_eq!(app.settings.profiles[0].preferences.tuner, global);

    app.playback.current = Some(tracks[1].clone());
    assert!(!app.tuner_pinned());
    assert_eq!(app.tuner_settings(), &global);
    key(&mut app, Key::Char('p'));
    key(&mut app, Key::Right);
    let second = TunerSettings {
        bass: 1,
        ..global.clone()
    };
    assert_eq!(app.tuner_settings(), &second);
    assert_eq!(app.settings.track_tuners[&tracks[0].path], pinned);
    saved(&mut app);
    drop(app);

    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.preferences.tuner, global);
    let change = app.store.create_playlist("Same file", Mode::Chaos).unwrap();
    let playlist = change
        .after
        .playlists
        .iter()
        .find(|p| p.name == "Same file")
        .unwrap()
        .id;
    app.store
        .copy_tracks(playlist, &[tracks[0].id], false)
        .unwrap();
    let copy = app
        .store
        .playlists()
        .unwrap()
        .into_iter()
        .find(|p| p.id == playlist)
        .unwrap()
        .entries
        .remove(0)
        .track;
    app.playback.current = Some(copy);
    assert!(app.tuner_pinned());
    assert_eq!(app.tuner_settings(), &pinned);
    let mut preferences = app.settings.profile_preferences();
    preferences.tuner = TunerSettings {
        speed: 80,
        ..Default::default()
    };
    app.settings.apply_preferences(preferences);
    assert_eq!(app.tuner_settings(), &pinned);
    app.playback.current = Some(tracks[1].clone());
    assert_eq!(app.tuner_settings(), &second);
    app.playback.current = None;
    assert_eq!(app.tuner_settings().speed, 80);
}

#[test]
fn reset_preserves_the_pin_and_unpin_restores_global_fx_without_changing_other_tracks() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let tracks = library_tracks(&mut app, &directory);
    app.settings.preferences.tuner = TunerSettings {
        speed: 130,
        bass: 25,
        ..Default::default()
    };
    let global = app.settings.preferences.tuner.clone();
    app.settings.track_tuners.insert(
        tracks[1].path.clone(),
        TunerSettings {
            pitch: 80,
            ..Default::default()
        },
    );
    let other = app.settings.track_tuners[&tracks[1].path].clone();
    app.playback.current = Some(tracks[0].clone());
    app.action(Action::PlaybackTuner).unwrap();
    key(&mut app, Key::Char('p'));
    key(&mut app, Key::Char('r'));
    assert!(app.tuner_pinned());
    assert_eq!(app.tuner_settings(), &TunerSettings::default());
    assert_eq!(app.settings.preferences.tuner, global);
    click(&mut app, |target| matches!(target, Target::TunerPin));
    assert!(!app.tuner_pinned());
    assert_eq!(app.tuner_settings(), &global);
    assert!(!app.settings.track_tuners.contains_key(&tracks[0].path));
    assert_eq!(app.settings.track_tuners[&tracks[1].path], other);
    // Drop must flush the latest pin/removal even without another UI tick.
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.preferences.tuner, global);
    assert!(!app.settings.track_tuners.contains_key(&tracks[0].path));
    assert_eq!(app.settings.track_tuners[&tracks[1].path], other);
}

#[test]
fn pin_requires_a_playing_track_and_legacy_track_fx_are_normalized_on_load() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.action(Action::PlaybackTuner).unwrap();
    render(&mut app, 120, 50);
    assert!(
        app.view
            .hits
            .iter()
            .any(|hit| matches!(hit.target, Target::TunerPin) && !hit.enabled)
    );
    key(&mut app, Key::Char('p'));
    assert!(app.settings.track_tuners.is_empty());
    assert!(!app.settings_saving());
    let mut legacy = serde_json::to_value(&app.settings).unwrap();
    legacy.as_object_mut().unwrap().remove("track_tuners");
    let mut settings: almavorn::model::Settings = serde_json::from_value(legacy).unwrap();
    assert!(settings.track_tuners.is_empty());
    let path = directory.0.join("Музыка.wav");
    settings.track_tuners.insert(
        path.clone(),
        TunerSettings {
            speed: 0,
            tempo: 900,
            pitch: 400,
            bass: 1000,
            reverse: true,
        },
    );
    app.store.save_settings(&settings).unwrap();
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(
        app.settings.track_tuners[&path],
        TunerSettings {
            speed: 50,
            tempo: 200,
            pitch: 200,
            bass: 100,
            reverse: true
        }
    );
}

#[test]
fn tuner_keys_wheel_drag_and_reset_preserve_equalizer_and_adjust_one_percent() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let equalizer = app.settings.preferences.equalizer.clone();
    key(&mut app, Key::F(5));
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::PlaybackTuner { selected: 0 })
    ));
    key(&mut app, Key::Right);
    assert_eq!(app.settings.preferences.tuner.speed, 101);
    key(&mut app, Key::Down);
    key(&mut app, Key::PageUp);
    assert_eq!(app.settings.preferences.tuner.tempo, 110);
    render(&mut app, 120, 50);
    let bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::TunerSlider(2, area) => Some(area),
            _ => None,
        })
        .unwrap();
    app.handle(Input::Scroll {
        x: bar.x,
        y: bar.y,
        delta: -99,
    });
    assert_eq!(app.settings.preferences.tuner.pitch, 101);
    assert_eq!(app.settings.preferences.tuner.tempo, 110);
    app.handle(Input::Click {
        x: bar.x,
        y: bar.y,
        double: false,
    });
    assert_eq!(app.settings.preferences.tuner.pitch, 50);
    app.handle(Input::Drag {
        x: bar.right() + 50,
        y: bar.y + 40,
    });
    assert_eq!(app.settings.preferences.tuner.pitch, 200);
    app.handle(Input::Release {
        x: bar.right() + 50,
        y: bar.y + 40,
    });
    assert!(app.view.workspace.gesture.is_none());
    key(&mut app, Key::Home);
    assert_eq!(app.settings.preferences.tuner.pitch, 100);
    key(&mut app, Key::Down);
    for _ in 0..120 {
        key(&mut app, Key::Right);
    }
    assert_eq!(app.settings.preferences.tuner.bass, 100);
    for _ in 0..120 {
        key(&mut app, Key::Left);
    }
    assert_eq!(app.settings.preferences.tuner.bass, 0);
    key(&mut app, Key::Char(' '));
    assert!(app.settings.preferences.tuner.reverse);
    key(&mut app, Key::Char('r'));
    assert_eq!(app.settings.preferences.tuner, TunerSettings::default());
    assert_eq!(app.settings.preferences.equalizer, equalizer);
    key(&mut app, Key::F(5));
    assert!(app.view.dialog.is_none());
}

#[test]
fn player_button_and_menu_expose_tuner_with_shortcut_and_changes_save_per_profile() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    assert!(
        app.command_items(CommandMenu::Player)
            .iter()
            .any(|item| item.enabled
                && item.label == "Настройщик проигрывания"
                && matches!(item.target, Target::Action(Action::PlaybackTuner)))
    );
    click(&mut app, |target| {
        matches!(target, Target::Action(Action::PlaybackTuner))
    });
    key(&mut app, Key::Right);
    key(&mut app, Key::Down);
    key(&mut app, Key::Left);
    key(&mut app, Key::Down);
    key(&mut app, Key::Right);
    key(&mut app, Key::Down);
    key(&mut app, Key::Right);
    click(&mut app, |target| matches!(target, Target::TunerReverse));
    let expected = app.settings.preferences.tuner.clone();
    assert_eq!(
        expected,
        TunerSettings {
            speed: 101,
            tempo: 99,
            pitch: 101,
            bass: 1,
            reverse: true
        }
    );
    saved(&mut app);
    let mut profile = app.settings.profile_preferences();
    profile.tuner = Default::default();
    app.settings.apply_preferences(profile);
    assert_eq!(app.settings.preferences.tuner, TunerSettings::default());
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.preferences.tuner, expected);
    assert_eq!(app.settings.profiles[0].preferences.tuner, expected);
    app.view.dialog = Some(Dialog::Commands {
        menu: CommandMenu::Player,
        selected: 0,
        anchor: ratatui::layout::Position::new(1, 1),
    });
    let terminal = render(&mut app, 140, 50);
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Настройщик проигрывания"));
    assert!(text.contains("F5"));
    key(&mut app, Key::Escape);
    click(&mut app, |target| {
        matches!(target, Target::Action(Action::PlaybackTuner))
    });
    click(&mut app, |target| matches!(target, Target::TunerReset));
    assert_eq!(app.settings.preferences.tuner, TunerSettings::default());
}

#[test]
fn tuner_keeps_sliders_accessible_in_both_languages_and_narrow_windows() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for language in [Language::Russian, Language::English] {
        app.settings.preferences.language = language;
        for (width, height) in [(140, 45), (70, 30), (32, 26), (32, 18)] {
            app.action(Action::PlaybackTuner).unwrap();
            for index in 0..4 {
                render(&mut app, width, height);
                assert!(
                    app.view.hits.iter().any(
                        |hit| matches!(hit.target, Target::TunerSlider(value, _) if value == index)
                    ),
                    "{width}x{height}: slider {index}"
                );
                assert!(
                    app.view
                        .hits
                        .iter()
                        .all(
                            |hit| hit.area.intersection(Rect::new(0, 0, width, height)) == hit.area
                        )
                );
                key(&mut app, Key::Tab);
            }
            assert!(
                app.view
                    .hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::TunerReverse))
            );
            assert!(
                app.view
                    .hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::TunerPin))
            );
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
fn legacy_settings_and_conflicting_shortcuts_migrate_without_changing_user_bindings() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let mut legacy = serde_json::to_value(store.settings().unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("tuner");
    for profile in legacy["profiles"].as_array_mut().unwrap() {
        profile["preferences"]
            .as_object_mut()
            .unwrap()
            .remove("tuner");
    }
    let mut settings: almavorn::model::Settings = serde_json::from_value(legacy).unwrap();
    assert_eq!(settings.preferences.tuner, TunerSettings::default());
    assert_eq!(
        settings.profiles[0].preferences.tuner,
        TunerSettings::default()
    );
    settings
        .preferences
        .bindings
        .retain(|binding| binding.action != Action::PlaybackTuner);
    let stop = settings
        .preferences
        .bindings
        .iter_mut()
        .find(|binding| binding.action == Action::Stop)
        .unwrap();
    stop.key = KeyPress::plain(Key::F(5));
    settings.preferences.tuner = TunerSettings {
        speed: 0,
        tempo: 900,
        pitch: 400,
        bass: 1000,
        reverse: true,
    };
    store.save_settings(&settings).unwrap();
    drop(store);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(
        app.settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == Action::Stop)
            .unwrap()
            .key,
        KeyPress::plain(Key::F(5))
    );
    assert_ne!(
        app.settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == Action::PlaybackTuner)
            .unwrap()
            .key,
        KeyPress::plain(Key::F(5))
    );
    assert_eq!(
        app.settings.preferences.tuner,
        TunerSettings {
            speed: 50,
            tempo: 200,
            pitch: 200,
            bass: 100,
            reverse: true
        }
    );
}
