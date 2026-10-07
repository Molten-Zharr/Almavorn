use almavorn::{
    app::{App, CommandMenu, Dialog, Target},
    equalizer::{EqualizerSettings, MAX_GAIN, MIN_GAIN, PRESETS, SLIDERS},
    input::{Action, Input, Key, KeyPress},
    model::Language,
    store::Store,
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
            "almavorn-equalizer-{}-{}",
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
fn equalizer_keys_wheel_and_dragging_change_only_the_targeted_gain() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    key(&mut app, Key::F(4));
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { selected: 0, .. })
    ));
    assert_eq!(app.settings.equalizer, EqualizerSettings::default());
    key(&mut app, Key::Char(' '));
    assert!(app.settings.equalizer.enabled);
    key(&mut app, Key::Right);
    key(&mut app, Key::Up);
    assert_eq!(app.settings.equalizer.bands[0], 1);
    render(&mut app, 120, 50);
    let bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::EqualizerGain {
                index: 1,
                area,
                vertical: true,
            } => Some(area),
            _ => None,
        })
        .unwrap();
    app.handle(Input::Scroll {
        x: bar.x,
        y: bar.y,
        delta: -99,
    });
    assert_eq!(app.settings.equalizer.bands[0], 2); // One dB per event.
    app.handle(Input::Click {
        x: bar.x,
        y: bar.y,
        double: false,
    });
    assert_eq!(app.settings.equalizer.bands[0], MAX_GAIN);
    app.handle(Input::Drag {
        x: bar.right() + 10,
        y: bar.bottom() + 10,
    });
    assert_eq!(app.settings.equalizer.bands[0], MIN_GAIN);
    app.handle(Input::Release {
        x: bar.right() + 10,
        y: bar.bottom() + 10,
    });
    assert!(app.view.workspace.gesture.is_none());
    assert_eq!(app.settings.equalizer.preamp, 0);
    assert_eq!(&app.settings.equalizer.bands[1..], &[0; 9]);
    key(&mut app, Key::Home);
    assert_eq!(app.settings.equalizer.bands, [0; 10]);
    for _ in 0..30 {
        key(&mut app, Key::Up);
    }
    assert_eq!(app.settings.equalizer.bands[0], MAX_GAIN);
    for _ in 0..30 {
        key(&mut app, Key::Down);
    }
    assert_eq!(app.settings.equalizer.bands[0], MIN_GAIN);
    key(&mut app, Key::Char('r'));
    assert_eq!(app.settings.equalizer.bands, [0; 10]);
    assert!(app.settings.equalizer.enabled);
    key(&mut app, Key::F(4));
    assert!(app.view.dialog.is_none());
}

#[test]
fn presets_apply_from_the_dropdown_and_shortcuts_and_custom_values_survive_restart() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    assert!(
        app.command_items(CommandMenu::Player)
            .iter()
            .any(|item| item.enabled
                && item.label == "Эквалайзер"
                && matches!(item.target, Target::Action(Action::Equalizer)))
    );
    click(&mut app, |target| {
        matches!(target, Target::Action(Action::Equalizer))
    });
    click(&mut app, |target| {
        matches!(target, Target::EqualizerPresets)
    });
    let terminal = render(&mut app, 120, 50);
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Танцевальный"));
    assert!(text.contains("Усиление баса"));
    assert_eq!(
        app.view
            .hits
            .iter()
            .filter(|hit| matches!(hit.target, Target::EqualizerPreset(_)))
            .count(),
        PRESETS.len()
    );
    click(&mut app, |target| {
        matches!(target, Target::EqualizerPreset(1))
    });
    assert_eq!(app.settings.equalizer.bands, PRESETS[1].bands);
    assert_eq!(app.settings.equalizer.preamp, PRESETS[1].preamp);
    assert!(!app.settings.equalizer.enabled); // Presets keep the on/off choice.
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { presets: false, .. })
    ));
    key(&mut app, Key::Char(' '));
    for index in 0..PRESETS.len() {
        key(&mut app, Key::Char(char::from(b'1' + index as u8)));
        assert_eq!(app.settings.equalizer.preset(), Some(index));
        assert!(app.settings.equalizer.enabled);
    }
    key(&mut app, Key::Up); // Customize the preamp of Vocal.
    assert_eq!(app.settings.equalizer.preset(), None);
    let terminal = render(&mut app, 120, 50);
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Пользовательский"));
    saved(&mut app);
    let expected = app.settings.equalizer.clone();
    let defaults = EqualizerSettings::default();
    let mut profile = app.settings.profile_preferences();
    profile.equalizer = defaults.clone();
    app.settings.apply_preferences(profile);
    assert_eq!(app.settings.equalizer, defaults);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.equalizer, expected);
    assert_eq!(app.settings.profiles[0].preferences.equalizer, expected);
}

#[test]
fn dropdown_dismissal_does_not_change_gains_and_escape_closes_it_before_the_equalizer() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.action(Action::Equalizer).unwrap();
    render(&mut app, 120, 50);
    let bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::EqualizerGain {
                index: 10, area, ..
            } => Some(area),
            _ => None,
        })
        .unwrap();
    key(&mut app, Key::Enter);
    render(&mut app, 120, 50);
    app.handle(Input::Click {
        x: bar.x,
        y: bar.y,
        double: false,
    });
    assert_eq!(app.settings.equalizer, EqualizerSettings::default());
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { presets: false, .. })
    ));
    key(&mut app, Key::Enter);
    key(&mut app, Key::Down);
    key(&mut app, Key::Enter);
    assert_eq!(app.settings.equalizer.preset(), Some(1));
    key(&mut app, Key::Enter);
    key(&mut app, Key::Escape);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { presets: false, .. })
    ));
    key(&mut app, Key::Escape);
    assert!(app.view.dialog.is_none());
}

#[test]
fn equalizer_adapts_to_small_windows_and_keeps_every_slider_reachable() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        for (width, height) in [(140, 45), (70, 30), (32, 26)] {
            app.action(Action::Equalizer).unwrap();
            for index in 0..SLIDERS {
                render(&mut app, width, height);
                assert!(app.view.hits.iter().any(|hit| matches!(hit.target, Target::EqualizerGain { index: value, .. } if value == index)));
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
            click(&mut app, |target| matches!(target, Target::CloseDialog));
        }
    }
}

#[test]
fn older_profiles_default_to_disabled_flat_eq_and_loaded_gains_are_clamped() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let settings = store.settings().unwrap();
    let mut legacy = serde_json::to_value(settings).unwrap();
    legacy.as_object_mut().unwrap().remove("equalizer");
    for profile in legacy["profiles"].as_array_mut().unwrap() {
        profile["preferences"]
            .as_object_mut()
            .unwrap()
            .remove("equalizer");
    }
    let mut settings: almavorn::model::Settings = serde_json::from_value(legacy).unwrap();
    assert_eq!(settings.equalizer, EqualizerSettings::default());
    assert_eq!(
        settings.profiles[0].preferences.equalizer,
        EqualizerSettings::default()
    );
    settings.equalizer.preamp = 100;
    settings.equalizer.bands = [-100; 10];
    store.save_settings(&settings).unwrap();
    drop(store);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.equalizer.preamp, MAX_GAIN);
    assert_eq!(app.settings.equalizer.bands, [MIN_GAIN; 10]);
}
