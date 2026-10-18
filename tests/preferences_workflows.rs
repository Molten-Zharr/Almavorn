use almavorn::{
    app::{App, SettingsPage},
    input::{Action, Input, Key, KeyPress},
    model::{Language, Mode, RepeatMode, Settings},
    preferences::ProfilePreferences,
    store::Store,
};
use serde_json::{Value, json};

// Captured from Settings at 5764bdc before introducing shared preferences.
const SAVED_SETTINGS: &str = include_str!("fixtures/settings-before-shared-preferences.json");

fn shared_settings(settings: &Settings) -> Value {
    let mut value = serde_json::to_value(settings).unwrap();
    for key in serde_json::to_value(&settings.preferences)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
    {
        value.as_object_mut().unwrap().remove(key);
    }
    value.as_object_mut().unwrap().remove("profiles");
    value
}

fn activate_profile(app: &mut App) {
    app.open_settings_page(SettingsPage::Profiles);
    for key in [Key::Tab, Key::Down, Key::Enter] {
        app.handle(Input::Key(KeyPress::plain(key)));
        assert!(!app.view.notice_error, "{}", app.view.notice);
    }
}

#[test]
fn previously_saved_settings_round_trip_with_the_same_flat_json_and_profile_shape() {
    let expected: Value = serde_json::from_str(SAVED_SETTINGS).unwrap();
    let settings: Settings = serde_json::from_str(SAVED_SETTINGS).unwrap();
    settings.validate_catalogs().unwrap();
    assert_eq!(settings.preferences.language, Language::English);
    assert_eq!(settings.preferences.volume, 0.45);
    assert_eq!(settings.profiles[0].preferences.language, Language::Russian);
    assert_eq!(settings.profiles[0].preferences.volume, 0.7);
    let round_trip: Value =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(round_trip, expected);

    let snapshot: Value =
        serde_json::from_str(&serde_json::to_string(&settings.profile_preferences()).unwrap())
            .unwrap();
    assert_eq!(snapshot.as_object().unwrap().len(), 14);
    for (field, value) in snapshot.as_object().unwrap() {
        assert_eq!(&expected[field], value, "Profile snapshot lost {field}");
    }
    assert!(expected.get("preferences").is_none());
}

#[test]
fn partial_settings_and_profiles_use_the_same_historical_defaults() {
    let empty: Settings = serde_json::from_value(json!({})).unwrap();
    let preferences: ProfilePreferences = serde_json::from_value(json!({})).unwrap();
    assert_eq!(
        serde_json::to_value(&empty.preferences).unwrap(),
        serde_json::to_value(&preferences).unwrap()
    );
    assert_eq!(preferences.language, Language::Russian);
    assert_eq!(preferences.volume, 0.7);
    assert_eq!(preferences.repeat, RepeatMode::Off);
    assert!(!preferences.shuffle);
    assert!(preferences.sorting_desk && preferences.scan_subfolders);
    assert_eq!(preferences.workspace.version, 3);
    assert_eq!(preferences.appearance.font_size, 16);

    let partial = json!({"language": "English", "volume": 0.25, "unknown_future_option": true});
    let settings: Settings = serde_json::from_value(partial.clone()).unwrap();
    let profile: ProfilePreferences = serde_json::from_value(partial).unwrap();
    assert_eq!(
        serde_json::to_value(settings.preferences).unwrap(),
        serde_json::to_value(profile).unwrap()
    );
}

#[test]
fn loading_an_old_database_and_activating_its_profile_preserves_shared_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let connection = rusqlite::Connection::open(&store.path).unwrap();
    connection
        .execute(
            "INSERT INTO settings(key,value) VALUES ('app',?1)",
            [SAVED_SETTINGS],
        )
        .unwrap();
    drop(connection);
    drop(store);

    let mut app = App::new(directory.path()).unwrap();
    assert_eq!(app.settings.preferences.volume, 0.45);
    assert!(app.settings.preferences.equalizer.enabled);
    assert!(app.settings.preferences.tuner.reverse);
    let shared = shared_settings(&app.settings);
    activate_profile(&mut app);
    assert_eq!(app.settings.preferences.volume, 0.7);
    assert!(!app.settings.preferences.equalizer.enabled);
    assert!(!app.settings.preferences.tuner.reverse);
    assert_eq!(app.settings.mode, Mode::Chaos);
    assert_eq!(shared_settings(&app.settings), shared);
    drop(app);

    let app = App::new(directory.path()).unwrap();
    assert_eq!(app.settings.preferences.volume, 0.7);
    assert_eq!(shared_settings(&app.settings), shared);
    let persisted = serde_json::to_value(app.store.settings().unwrap()).unwrap();
    assert!(persisted.get("preferences").is_none());
    assert_eq!(
        persisted["profiles"][0]["preferences"]["volume"],
        json!(0.7f32)
    );
}

#[test]
fn startup_and_profile_activation_normalize_audio_layout_and_shortcuts_identically() {
    let directory = tempfile::tempdir().unwrap();
    let mut settings: Settings = serde_json::from_str(SAVED_SETTINGS).unwrap();
    settings.preferences.volume = 2.0;
    settings.preferences.equalizer.preamp = 100;
    settings.preferences.equalizer.bands = [-100; 10];
    settings.preferences.tuner.speed = 0;
    settings.preferences.tuner.pitch = 1000;
    settings.preferences.tuner.bass = 200;
    settings.preferences.workspace.version = 1;
    settings.preferences.bindings[0].key = KeyPress::plain(Key::F(4));
    settings.profiles[0].preferences = settings.preferences.clone();
    Store::open(directory.path())
        .unwrap()
        .save_settings(&settings)
        .unwrap();

    let mut app = App::new(directory.path()).unwrap();
    let normalized = serde_json::to_value(&app.settings.preferences).unwrap();
    assert_eq!(app.settings.preferences.volume, 1.0);
    assert_eq!(app.settings.preferences.equalizer.preamp, 12);
    assert_eq!(app.settings.preferences.equalizer.bands, [-12; 10]);
    assert_eq!(app.settings.preferences.tuner.speed, 50);
    assert_eq!(app.settings.preferences.tuner.pitch, 200);
    assert_eq!(app.settings.preferences.tuner.bass, 100);
    assert_eq!(app.settings.preferences.workspace.version, 3);
    for binding in almavorn::input::default_bindings() {
        assert!(
            app.settings
                .preferences
                .bindings
                .iter()
                .any(|saved| saved.action == binding.action)
        );
    }
    let equalizer = app
        .settings
        .preferences
        .bindings
        .iter()
        .find(|binding| binding.action == Action::Equalizer)
        .unwrap();
    assert_ne!(equalizer.key, KeyPress::plain(Key::F(4)));
    activate_profile(&mut app);
    assert_eq!(
        serde_json::to_value(&app.settings.preferences).unwrap(),
        normalized
    );
}

#[test]
fn synchronizing_a_profile_copies_every_preference_and_keeps_the_snapshot_independent() {
    let mut settings: Settings = serde_json::from_str(SAVED_SETTINGS).unwrap();
    let shared = shared_settings(&settings);
    let expected = serde_json::to_value(&settings.preferences).unwrap();
    settings.sync_active_profile();
    assert_eq!(
        serde_json::to_value(&settings.profiles[0].preferences).unwrap(),
        expected
    );
    settings.preferences.appearance.font_size = 19;
    settings.preferences.equalizer.bands[0] = -9;
    settings.preferences.tuner.speed = 90;
    settings.preferences.bindings.clear();
    assert_eq!(
        serde_json::to_value(&settings.profiles[0].preferences).unwrap(),
        expected
    );
    assert_eq!(shared_settings(&settings), shared);
}
