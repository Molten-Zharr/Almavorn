use almavorn::{
    app::{App, CommandMenu, Dialog, Target},
    input::{Action, Input, Key, KeyPress},
    model::Language,
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

#[test]
fn tuner_keys_wheel_drag_and_reset_preserve_equalizer_and_adjust_one_percent() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let equalizer = app.settings.equalizer.clone();
    key(&mut app, Key::F(5));
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::PlaybackTuner { selected: 0 })
    ));
    key(&mut app, Key::Right);
    assert_eq!(app.settings.tuner.speed, 101);
    key(&mut app, Key::Down);
    key(&mut app, Key::PageUp);
    assert_eq!(app.settings.tuner.tempo, 110);
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
    assert_eq!(app.settings.tuner.pitch, 101);
    assert_eq!(app.settings.tuner.tempo, 110);
    app.handle(Input::Click {
        x: bar.x,
        y: bar.y,
        double: false,
    });
    assert_eq!(app.settings.tuner.pitch, 50);
    app.handle(Input::Drag {
        x: bar.right() + 50,
        y: bar.y + 40,
    });
    assert_eq!(app.settings.tuner.pitch, 200);
    app.handle(Input::Release {
        x: bar.right() + 50,
        y: bar.y + 40,
    });
    assert!(app.view.workspace.gesture.is_none());
    key(&mut app, Key::Home);
    assert_eq!(app.settings.tuner.pitch, 100);
    key(&mut app, Key::Down);
    for _ in 0..120 {
        key(&mut app, Key::Right);
    }
    assert_eq!(app.settings.tuner.bass, 100);
    for _ in 0..120 {
        key(&mut app, Key::Left);
    }
    assert_eq!(app.settings.tuner.bass, 0);
    key(&mut app, Key::Char(' '));
    assert!(app.settings.tuner.reverse);
    key(&mut app, Key::Char('r'));
    assert_eq!(app.settings.tuner, TunerSettings::default());
    assert_eq!(app.settings.equalizer, equalizer);
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
    let expected = app.settings.tuner.clone();
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
    assert_eq!(app.settings.tuner, TunerSettings::default());
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.tuner, expected);
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
    assert_eq!(app.settings.tuner, TunerSettings::default());
}

#[test]
fn tuner_keeps_sliders_accessible_in_both_languages_and_narrow_windows() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
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
    assert_eq!(settings.tuner, TunerSettings::default());
    assert_eq!(
        settings.profiles[0].preferences.tuner,
        TunerSettings::default()
    );
    settings
        .bindings
        .retain(|binding| binding.action != Action::PlaybackTuner);
    let stop = settings
        .bindings
        .iter_mut()
        .find(|binding| binding.action == Action::Stop)
        .unwrap();
    stop.key = KeyPress::plain(Key::F(5));
    settings.tuner = TunerSettings {
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
            .bindings
            .iter()
            .find(|binding| binding.action == Action::Stop)
            .unwrap()
            .key,
        KeyPress::plain(Key::F(5))
    );
    assert_ne!(
        app.settings
            .bindings
            .iter()
            .find(|binding| binding.action == Action::PlaybackTuner)
            .unwrap()
            .key,
        KeyPress::plain(Key::F(5))
    );
    assert_eq!(
        app.settings.tuner,
        TunerSettings {
            speed: 50,
            tempo: 200,
            pitch: 200,
            bass: 100,
            reverse: true
        }
    );
}
