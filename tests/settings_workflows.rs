use almavorn::{
    app::{App, Dialog, SettingsFocus, SettingsPage, Target},
    input::{Action, Input, Key, KeyPress},
    model::Language,
    preferences::{BRAND_PALETTE, BorderWeight, Corners, FontFace, NamedPalette, color_roles},
    store::Store,
    ui,
};
use ratatui::{Terminal, backend::TestBackend, style::Color};
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
            "almavorn-settings-{}-{}",
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
fn ctrl(app: &mut App, c: char) {
    app.handle(Input::Key(KeyPress {
        key: Key::Char(c),
        ctrl: true,
        alt: false,
        shift: false,
    }));
}
fn until(app: &mut App, predicate: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate(app) {
        assert!(Instant::now() < deadline, "Timed out: {}", app.notice);
        app.tick();
        std::thread::sleep(Duration::from_millis(3));
    }
}
fn saved(app: &mut App) {
    let expected = serde_json::to_value(&app.settings).unwrap();
    until(app, |app| {
        serde_json::to_value(app.store.settings().unwrap()).unwrap() == expected
    });
}
fn submit(app: &mut App, text: &str) {
    assert!(app.accepts_text());
    ctrl(app, 'a');
    app.handle(Input::Text(text.into()));
    key(app, Key::Enter);
    assert!(!app.notice_error, "{}", app.notice);
}
fn select_row(app: &mut App, row: usize) {
    if app.settings_focus() == SettingsFocus::Menu {
        key(app, Key::Tab);
    }
    key(app, Key::Home);
    for _ in 0..row {
        key(app, Key::Down);
    }
}
fn click(app: &mut App, predicate: impl Fn(&Target) -> bool) {
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    let area = app
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
}
fn click_row(app: &mut App, row: usize) {
    select_row(app, row);
    click(
        app,
        |target| matches!(target,Target::Setting(index) if *index == row),
    );
}
fn rendered(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    terminal
}
fn screen_text(terminal: &Terminal<TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn brand_palette_is_first_and_applies_all_approved_colors() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.palettes[0].name, "Molten-Zharr");
    assert_eq!(
        app.settings.appearance.palette_ids,
        [BRAND_PALETTE, BRAND_PALETTE]
    );
    assert_eq!(app.settings.appearance.font, FontFace::FiraCodeBold);
    let palette = app.settings.current_palette();
    assert_eq!(palette.colors.len(), 16);
    assert_eq!(color_roles().len(), 16);
    for (key, color) in [
        ("background", [0, 0, 0]),
        ("text", [248, 248, 242]),
        ("active", [255, 0, 0]),
        ("selection_accent", [255, 141, 52]),
        ("error", [215, 0, 0]),
        ("selected_file_background", [82, 69, 73]),
    ] {
        assert_eq!(palette.color(key), color);
    }
    key(&mut app, Key::F(2));
    let terminal = rendered(&mut app, 120, 50);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0, 0, 0));
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(0, 0)].symbol(), "┌");
}

#[test]
fn pages_keep_mouse_keyboard_descriptions_and_footer_available() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    key(&mut app, Key::F(2));
    assert_eq!(app.settings_page(), SettingsPage::General);
    key(&mut app, Key::Down);
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    key(&mut app, Key::Tab);
    assert_eq!(app.settings_focus(), SettingsFocus::Parameters);
    key(&mut app, Key::F(1));
    assert!(matches!(app.dialog, Some(Dialog::SettingsHelp { .. })));
    key(&mut app, Key::Escape);
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    click(&mut app, |target| {
        matches!(target, Target::SettingsPage(SettingsPage::General))
    });
    click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(1, 1))
    });
    assert!((app.settings.volume - 0.75).abs() < 0.001);
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        for page in SettingsPage::ALL {
            app.open_settings_page(page);
            for (width, height) in [(120, 50), (70, 30), (44, 22), (20, 8)] {
                let terminal = rendered(&mut app, width, height);
                assert!(
                    app.hits
                        .iter()
                        .any(|hit| hit.enabled && matches!(hit.target, Target::CloseDialog)),
                    "{page:?} {width}x{height}"
                );
                assert!(
                    app.hits
                        .iter()
                        .all(|hit| hit.area.right() <= width && hit.area.bottom() <= height)
                );
                if width >= 70 {
                    let text = screen_text(&terminal);
                    assert!(
                        text.contains("Tab:") && text.contains("Enter:") && text.contains("Esc:"),
                        "{page:?}: {text}"
                    );
                }
                if page == SettingsPage::Palettes && width == 120 {
                    let text = screen_text(&terminal);
                    assert!(
                        text.contains("[#F8F8F2]") && text.contains("[#F9F9FE]"),
                        "{text}"
                    );
                }
                assert!(!app.hits.iter().any(|hit| matches!(
                    hit.target,
                    Target::Playlist(_) | Target::Track(_) | Target::Mode(_)
                )));
            }
        }
    }
}

#[test]
fn shortcuts_support_mouse_assignment_and_preserve_conflict_protection() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let index = app
        .settings
        .bindings
        .iter()
        .position(|b| b.action == Action::Settings)
        .unwrap();
    app.open_settings_page(SettingsPage::Shortcuts);
    click_row(&mut app, index + 1);
    let original = app.settings.bindings[index].key;
    click(&mut app, |target| {
        matches!(target, Target::BindingKey(Key::F(1)))
    });
    assert!(app.notice_error);
    assert_eq!(app.settings.bindings[index].key, original);
    assert!(matches!(app.dialog, Some(Dialog::CaptureBinding { .. })));
    click(&mut app, |target| {
        matches!(target, Target::BindingKey(Key::Up))
    });
    assert!(app.notice_error);
    assert_eq!(app.settings.bindings[index].key, original);
    click(&mut app, |target| {
        matches!(target, Target::BindingModifier(0))
    });
    click(&mut app, |target| {
        matches!(target, Target::BindingModifier(1))
    });
    click(&mut app, |target| {
        matches!(target, Target::BindingKey(Key::Char('j')))
    });
    assert_eq!(
        app.settings.bindings[index].key,
        KeyPress {
            key: Key::Char('j'),
            ctrl: true,
            alt: true,
            shift: false
        }
    );
    assert_eq!(app.settings_page(), SettingsPage::Shortcuts);
    assert!(matches!(app.dialog, Some(Dialog::Settings { .. })));
    key(&mut app, Key::Escape);
    app.handle(Input::Key(app.settings.bindings[index].key));
    assert_eq!(app.settings_page(), SettingsPage::General);
    assert!(matches!(app.dialog, Some(Dialog::Settings { .. })));
    saved(&mut app);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.bindings[index].key.label(), "Ctrl+Alt+J");
}

#[test]
fn settings_editors_keep_context_and_mouse_return_when_the_window_shrinks() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let mut dialogs = Vec::new();
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::Insert);
    dialogs.push(app.dialog.clone().unwrap());
    submit(&mut app, "Second profile");
    key(&mut app, Key::Delete);
    dialogs.push(app.dialog.clone().unwrap());
    key(&mut app, Key::Escape);
    key(&mut app, Key::F(1));
    dialogs.push(app.dialog.clone().unwrap());
    key(&mut app, Key::Escape);
    app.open_settings_page(SettingsPage::Shortcuts);
    click_row(&mut app, 1);
    dialogs.push(app.dialog.clone().unwrap());
    for dialog in dialogs {
        for (width, height) in [(120, 50), (70, 30), (44, 22), (20, 8)] {
            app.dialog = Some(dialog.clone());
            rendered(&mut app, width, height);
            assert!(
                app.hits
                    .iter()
                    .all(|hit| hit.area.right() <= width && hit.area.bottom() <= height)
            );
            assert!(
                !app.hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::SettingsPage(_)))
            );
            let close = app
                .hits
                .iter()
                .find(|hit| hit.enabled && matches!(hit.target, Target::CloseDialog))
                .unwrap()
                .area;
            app.handle(Input::Click {
                x: close.x,
                y: close.y,
                double: false,
            });
            assert!(matches!(app.dialog, Some(Dialog::Settings { .. })));
        }
    }
}

#[test]
fn profiles_clone_autosave_switch_rename_delete_and_survive_restart() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let library = app.store.snapshot("order").unwrap();
    let mode = app.settings.mode;
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::Insert);
    submit(&mut app, "Night");
    let id = app.settings.active_profile.clone();
    assert_eq!(app.settings.profiles.len(), 2);
    app.open_settings_page(SettingsPage::General);
    select_row(&mut app, 1);
    key(&mut app, Key::Right);
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::F(3));
    submit(&mut app, "Evening");
    let profile = app.settings.profiles.iter().find(|p| p.id == id).unwrap();
    assert_eq!(profile.name, "Evening");
    assert!((profile.preferences.volume - 0.75).abs() < 0.001);
    select_row(&mut app, 0);
    key(&mut app, Key::Left);
    click_row(&mut app, 1);
    assert_eq!(app.settings.active_profile, "default");
    assert!((app.settings.volume - 0.7).abs() < 0.001);
    assert_eq!(app.settings.mode, mode);
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    click_row(&mut app, 1);
    assert_eq!(app.settings.active_profile, id);
    key(&mut app, Key::Delete);
    assert!(matches!(app.dialog, Some(Dialog::ConfirmSettings { .. })));
    key(&mut app, Key::Escape);
    assert_eq!(app.settings.profiles.len(), 2);
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.active_profile, id);
    assert!((app.settings.volume - 0.75).abs() < 0.001);
    app.open_settings_page(SettingsPage::Profiles);
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    key(&mut app, Key::Delete);
    key(&mut app, Key::Enter);
    assert_eq!(app.settings.profiles.len(), 1);
    assert_eq!(app.settings.active_profile, "default");
    assert_eq!(app.store.snapshot("order").unwrap(), library);
    saved(&mut app);
}

#[test]
fn palettes_crud_validate_colors_roundtrip_and_preserve_references() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Palettes);
    key(&mut app, Key::Insert);
    submit(&mut app, "Custom");
    let id = app.settings.palettes.last().unwrap().id.clone();
    let original = app.settings.palettes.last().unwrap().color("background");
    click_row(&mut app, 3);
    ctrl(&mut app, 'a');
    app.handle(Input::Text("invalid".into()));
    key(&mut app, Key::Enter);
    assert!(app.notice_error);
    assert_eq!(
        app.settings.palettes.last().unwrap().color("background"),
        original
    );
    assert!(app.accepts_text());
    submit(&mut app, "#102030");
    assert_eq!(
        app.settings.palettes.last().unwrap().color("background"),
        [16, 32, 48]
    );
    click_row(&mut app, 1);
    assert_eq!(app.settings.current_palette().id, id);
    key(&mut app, Key::F(3));
    submit(&mut app, "Personal");
    assert_eq!(app.settings.current_palette().id, id);
    let colors = app.settings.current_palette().colors.clone();
    let path = directory.0.join("palette.json");
    ctrl(&mut app, 'e');
    submit(&mut app, path.to_str().unwrap());
    until(&mut app, |_| {
        fs::read(&path)
            .ok()
            .is_some_and(|bytes| NamedPalette::import_json(&bytes).is_ok())
    });
    let bytes = fs::read(&path).unwrap();
    until(&mut app, |app| !app.settings_file_busy());
    assert_eq!(NamedPalette::import_json(&bytes).unwrap().colors, colors);
    let before = app.settings.palettes.len();
    ctrl(&mut app, 'i');
    submit(&mut app, path.to_str().unwrap());
    until(&mut app, |app| app.settings.palettes.len() == before + 1);
    assert_ne!(app.settings.palettes.last().unwrap().id, id);
    assert_eq!(app.settings.palettes.last().unwrap().name, "Personal (2)");
    assert_eq!(app.settings.palettes.last().unwrap().colors, colors);
    // Deleting the selected imported palette cannot affect the original or overwrite its export.
    key(&mut app, Key::Delete);
    key(&mut app, Key::Enter);
    assert_eq!(app.settings.palettes.len(), before);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    // Select the original custom palette and delete it while a profile references it.
    select_row(&mut app, 0);
    key(&mut app, Key::Delete);
    key(&mut app, Key::Enter);
    assert!(!app.settings.palettes.iter().any(|p| p.id == id));
    app.settings.validate_catalogs().unwrap();
    assert!(
        app.settings
            .profiles
            .iter()
            .all(|p| !p.preferences.appearance.palette_ids.contains(&id))
    );
    saved(&mut app);
}

#[test]
fn palette_import_failure_and_export_collision_keep_existing_data() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let path = directory.0.join("protected.json");
    fs::write(&path, b"keep this file").unwrap();
    app.open_settings_page(SettingsPage::Palettes);
    let palettes = app.settings.palettes.clone();
    ctrl(&mut app, 'i');
    submit(&mut app, path.to_str().unwrap());
    until(&mut app, |app| app.notice_error);
    assert_eq!(app.settings.palettes, palettes);
    assert_eq!(fs::read(&path).unwrap(), b"keep this file");
    ctrl(&mut app, 'e');
    submit(&mut app, path.to_str().unwrap());
    until(&mut app, |app| app.notice_error);
    assert_eq!(fs::read(&path).unwrap(), b"keep this file");
    assert_eq!(app.settings.palettes, palettes);
    assert!(matches!(app.dialog, Some(Dialog::Settings { .. })));
}

#[test]
fn theme_presets_store_font_geometry_and_palette_with_full_crud() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Typography);
    click_row(&mut app, 0);
    assert_eq!(app.settings.appearance.font, FontFace::FiraCode);
    click_row(&mut app, 1);
    submit(&mut app, "24");
    select_row(&mut app, 2);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.appearance.borders, BorderWeight::Double);
    app.open_settings_page(SettingsPage::Themes);
    key(&mut app, Key::Insert);
    submit(&mut app, "Large");
    key(&mut app, Key::F(3));
    submit(&mut app, "Comfort");
    let id = app.settings.presets.last().unwrap().id.clone();
    app.open_settings_page(SettingsPage::Typography);
    click_row(&mut app, 1);
    submit(&mut app, "12");
    app.open_settings_page(SettingsPage::Themes);
    click_row(&mut app, 1);
    assert_eq!(app.settings.appearance.font_size, 24);
    assert_eq!(app.settings.appearance.borders, BorderWeight::Double);
    click_row(&mut app, 6);
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    let preset = app.settings.presets.iter().find(|p| p.id == id).unwrap();
    assert_eq!(preset.name, "Comfort");
    assert_eq!(preset.style.font_size, 24);
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
    for _ in 0..4 {
        key(&mut app, Key::Right);
    }
    key(&mut app, Key::Delete);
    key(&mut app, Key::Enter);
    assert!(!app.settings.presets.iter().any(|p| p.id == id));
    assert_eq!(app.settings.appearance.font_size, 24);
    saved(&mut app);
}

#[test]
fn geometry_changes_rendered_borders_and_help_keeps_context() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Typography);
    for (weight, corners, symbol) in [
        (BorderWeight::Single, Corners::Square, "┌"),
        (BorderWeight::Single, Corners::Rounded, "╭"),
        (BorderWeight::Double, Corners::Square, "╔"),
        (BorderWeight::Thick, Corners::Square, "┏"),
    ] {
        app.settings.appearance.borders = weight;
        app.settings.appearance.corners = corners;
        assert_eq!(
            rendered(&mut app, 80, 30).backend().buffer()[(0, 0)].symbol(),
            symbol
        );
    }
    app.settings.appearance.borders = BorderWeight::None;
    let terminal = rendered(&mut app, 80, 30);
    assert_ne!(terminal.backend().buffer()[(0, 0)].symbol(), "┌");
    select_row(&mut app, 1);
    key(&mut app, Key::F(1));
    assert!(matches!(app.dialog, Some(Dialog::SettingsHelp { .. })));
    click(&mut app, |target| matches!(target, Target::CloseDialog));
    assert_eq!(app.settings_page(), SettingsPage::Typography);
    click_row(&mut app, 1);
    submit(&mut app, "32");
    select_row(&mut app, 1);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.appearance.font_size, 32);
    click_row(&mut app, 1);
    ctrl(&mut app, 'a');
    app.handle(Input::Text("1000".into()));
    key(&mut app, Key::Enter);
    assert!(app.notice_error);
    assert_eq!(app.settings.appearance.font_size, 32);
    key(&mut app, Key::Escape);
    assert_eq!(app.settings_page(), SettingsPage::Typography);
}

#[test]
fn legacy_settings_migrate_without_resetting_custom_colors() {
    let directory = Directory::new();
    let store = Store::open(&directory.0).unwrap();
    let mut old = serde_json::to_value(store.settings().unwrap()).unwrap();
    drop(store);
    for key in [
        "appearance",
        "palettes",
        "presets",
        "profiles",
        "active_profile",
        "next_settings_id",
    ] {
        old.as_object_mut().unwrap().remove(key);
    }
    old["themes"][0]["accent"] = serde_json::json!([12, 34, 56]);
    old["volume"] = serde_json::json!(0.45);
    let connection = duckdb::Connection::open(directory.0.join("almavorn.duckdb")).unwrap();
    connection
        .execute(
            "INSERT INTO settings(key,value) VALUES ('app',?1)",
            [old.to_string()],
        )
        .unwrap();
    drop(connection);
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.current_palette().color("active"), [12, 34, 56]);
    assert_eq!(app.settings.palettes[0].id, BRAND_PALETTE);
    assert_eq!(app.settings.profiles.len(), 1);
    assert!((app.settings.volume - 0.45).abs() < 0.001);
    let count = app.settings.palettes.len();
    app.action(Action::VolumeDown).unwrap();
    saved(&mut app);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.palettes.len(), count);
    assert_eq!(app.settings.current_palette().color("active"), [12, 34, 56]);
    assert!((app.settings.volume - 0.4).abs() < 0.001);
}

#[test]
fn palette_files_reject_missing_roles_invalid_values_and_future_versions() {
    let brand =
        NamedPalette::import_json(include_bytes!("../assets/brand/molten-zharr.json")).unwrap();
    assert_eq!(brand.name, "Molten-Zharr");
    let mut exported: serde_json::Value =
        serde_json::from_slice(&brand.export_json().unwrap()).unwrap();
    exported["version"] = serde_json::json!(99);
    assert!(NamedPalette::import_json(&serde_json::to_vec(&exported).unwrap()).is_err());
    exported["version"] = serde_json::json!(1);
    exported["palette"]["colors"]["active"] = serde_json::json!([999, 0, 0]);
    assert!(NamedPalette::import_json(&serde_json::to_vec(&exported).unwrap()).is_err());
    exported["palette"]["colors"]
        .as_object_mut()
        .unwrap()
        .remove("active");
    assert!(NamedPalette::import_json(&serde_json::to_vec(&exported).unwrap()).is_err());
}
