use almavorn::{
    app::{
        App, Browser, CommandMenu, Dialog, Focus, SettingsCatalog, SettingsFocus, SettingsPage,
        TextDialog, TextPurpose,
    },
    input::{Action, Input, Key, KeyPress},
    model::{Language, Mode, Track},
    store::Store,
    workspace::{Gesture, Panel, PanelRow, panel_rows},
};
use ratatui::layout::{Position, Rect};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

fn press(app: &mut App, key: KeyPress) {
    app.handle(Input::Key(key));
}

fn key(app: &mut App, key: Key) {
    press(app, KeyPress::plain(key));
}

fn ctrl(key: Key) -> KeyPress {
    KeyPress {
        key,
        ctrl: true,
        alt: false,
        shift: false,
    }
}

fn until(app: &mut App, ready: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(app) {
        assert!(Instant::now() < deadline, "Timed out: {}", app.view.notice);
        app.tick();
        assert!(!app.view.notice_error, "{}", app.view.notice);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn text(purpose: TextPurpose, value: &str) -> Dialog {
    Dialog::Text(TextDialog {
        purpose,
        text: value.into(),
        keyboard: Language::Russian,
        upper: false,
        selected_all: false,
        color_picker: None,
    })
}

fn active_browser(app: &App) -> &Browser {
    let Some(Dialog::Browser(browser)) = &app.view.dialog else {
        panic!("Expected browser");
    };
    browser
}

fn open_folder_picker(app: &mut App, music: &Path) {
    app.view.dialog = Some(text(
        TextPurpose::CreateFromFolder {
            folder: music.into(),
            group_subfolders: false,
        },
        "Draft",
    ));
    press(app, ctrl(Key::Char('f')));
    until(app, App::browser_ready);
    assert_eq!(active_browser(app).directory, music);
}

fn music_folders(root: &Path) -> std::path::PathBuf {
    let music = root.join("music");
    fs::create_dir_all(music.join("a")).unwrap();
    fs::create_dir(music.join("b")).unwrap();
    music
}

fn seeded(root: &Path) -> (App, Vec<i64>) {
    let mut store = Store::open(root).unwrap();
    for name in ["A", "B", "C"] {
        store.create_playlist(name, Mode::Chaos).unwrap();
    }
    let ids = store
        .playlists()
        .unwrap()
        .iter()
        .filter(|playlist| playlist.mode == Mode::Chaos)
        .map(|playlist| playlist.id)
        .collect();
    drop(store);
    let mut app = App::new(root).unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    (app, ids)
}

#[test]
fn dialogs_preserve_their_shortcut_policies() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    let track = Track {
        id: 1,
        path: directory.path().join("track.wav"),
        title: "Track".into(),
        artist: String::new(),
        album: String::new(),
        duration_ms: 1000,
        tags: "{}".into(),
    };
    let dialogs = [
        text(TextPurpose::Create, "Draft"),
        Dialog::Help { offset: 0 },
        Dialog::Metadata { track, offset: 0 },
        Dialog::Settings { selected: 0 },
        Dialog::SettingsHelp {
            title: "Help".into(),
            description: "Details".into(),
            offset: 0,
        },
        Dialog::ConfirmSettings {
            catalog: SettingsCatalog::Palette,
            id: app.settings.palettes[0].id.clone(),
            name: "Palette".into(),
        },
        Dialog::RemoveEntry {
            playlist: 1,
            entry: 1,
            title: "Track".into(),
        },
        Dialog::RemovePlaylist {
            playlist: 1,
            name: "Playlist".into(),
            confirm: false,
        },
        Dialog::Transfer {
            ids: vec![1],
            selected: 0,
        },
        Dialog::Panels {
            selected: 0,
            expanded: Vec::new(),
        },
        Dialog::Folders {
            playlist: 1,
            selected: 0,
        },
        Dialog::ComposePlaylists {
            selected: 0,
            ids: Vec::new(),
        },
        Dialog::Equalizer {
            selected: 0,
            presets: false,
            preset_selected: 0,
        },
        Dialog::PlaybackTuner { selected: 0 },
    ];
    for dialog in dialogs {
        app.view.dialog = Some(dialog);
        key(&mut app, Key::Char('ь')); // Physical M in the Russian layout.
        key(&mut app, Key::Char('q'));
        assert_eq!(app.settings.mode, Mode::Order);
        assert!(!app.view.quit);
        assert!(app.view.dialog.is_some());
    }
    app.action(Action::Search).unwrap();
    key(&mut app, Key::Char('ь'));
    assert!(matches!(app.view.dialog, Some(Dialog::Search(_))));
    assert_eq!(app.settings.mode, Mode::Order);

    app.view.dialog = Some(Dialog::Statistics);
    key(&mut app, Key::Char('ь'));
    assert_eq!(app.settings.mode, Mode::Order);
    key(&mut app, Key::Char('q'));
    assert!(app.view.quit); // Statistics explicitly allows playback and Quit shortcuts.
    app.view.quit = false;

    app.view.dialog = Some(Dialog::Commands {
        menu: CommandMenu::Application,
        selected: 0,
        anchor: Position::new(0, 0),
    });
    key(&mut app, Key::Char('ь'));
    assert_eq!(app.settings.mode, Mode::Chaos);
    assert!(app.view.dialog.is_none());
}

#[test]
fn text_selection_and_backspace_take_priority_over_global_shortcuts() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    app.settings
        .bindings
        .iter_mut()
        .find(|binding| binding.action == Action::Quit)
        .unwrap()
        .key = ctrl(Key::Char('a'));
    app.view.dialog = Some(text(TextPurpose::Create, "Музыка🎵"));
    press(&mut app, ctrl(Key::Char('a')));
    assert!(matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if dialog.selected_all));
    assert!(!app.view.quit);
    key(&mut app, Key::Backspace);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if dialog.text.is_empty() && !dialog.selected_all)
    );
    app.handle(Input::Text("Музыка🎵".into()));
    key(&mut app, Key::Backspace);
    assert!(matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if dialog.text == "Музыка"));
    key(&mut app, Key::Escape);
    assert!(app.view.dialog.is_none());
}

#[test]
fn binding_capture_rejects_reserved_and_conflicting_keys_and_normalizes_assignment() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    let index = app
        .settings
        .bindings
        .iter()
        .position(|binding| binding.action == Action::Settings)
        .unwrap();
    let original = app.settings.bindings[index].key;
    app.open_settings_page(SettingsPage::Shortcuts);
    app.view.dialog = Some(Dialog::CaptureBinding { index });
    for invalid in [Key::Up, Key::F(1)] {
        key(&mut app, invalid);
        assert!(app.view.notice_error);
        assert_eq!(app.settings.bindings[index].key, original);
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::CaptureBinding { .. })
        ));
    }
    key(&mut app, Key::Escape);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
    assert_eq!(app.settings.bindings[index].key, original);
    app.view.dialog = Some(Dialog::CaptureBinding { index });
    press(
        &mut app,
        KeyPress {
            key: Key::Char('о'),
            ctrl: true,
            alt: true,
            shift: false,
        },
    );
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
    assert_eq!(app.settings_focus(), SettingsFocus::Parameters);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
}

#[test]
fn escape_cancels_drag_before_dismissing_equalizer_presets_and_dialog() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    app.action(Action::Equalizer).unwrap();
    key(&mut app, Key::Enter);
    app.view.workspace.gesture = Some(Gesture::EqualizerGain {
        index: 0,
        area: Rect::new(0, 0, 20, 1),
        vertical: false,
    });
    key(&mut app, Key::Escape);
    assert!(app.view.workspace.gesture.is_none());
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { presets: true, .. })
    ));
    key(&mut app, Key::Escape);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Equalizer { presets: false, .. })
    ));
    key(&mut app, Key::Escape);
    assert!(app.view.dialog.is_none());
}

#[test]
fn browser_navigation_and_escape_restore_the_playlist_name_draft() {
    let directory = tempfile::tempdir().unwrap();
    let music = music_folders(directory.path());
    let mut app = App::new(directory.path()).unwrap();
    open_folder_picker(&mut app, &music);
    key(&mut app, Key::End);
    assert_eq!(active_browser(&app).selected, 1);
    key(&mut app, Key::Home);
    assert_eq!(active_browser(&app).selected, 0);
    key(&mut app, Key::Enter);
    until(&mut app, App::browser_ready);
    assert_eq!(active_browser(&app).directory, music.join("a"));
    key(&mut app, Key::Backspace);
    until(&mut app, App::browser_ready);
    assert_eq!(active_browser(&app).directory, music);
    key(&mut app, Key::Char('f')); // A name picker keeps its draft rather than toggling Add folder.
    assert!(matches!(app.view.dialog, Some(Dialog::Browser(_))));
    key(&mut app, Key::Escape);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if dialog.text == "Draft" && matches!(&dialog.purpose, TextPurpose::CreateFromFolder { folder, .. } if folder == &music))
    );
}

#[test]
fn folder_acceptance_distinguishes_selected_folder_from_shift_enter_current_folder() {
    let directory = tempfile::tempdir().unwrap();
    let music = music_folders(directory.path());
    let mut app = App::new(directory.path()).unwrap();
    for shift in [false, true] {
        app.view.dialog = Some(text(
            TextPurpose::CreateFromFolder {
                folder: music.clone(),
                group_subfolders: false,
            },
            "Draft",
        ));
        press(&mut app, ctrl(Key::Char('g')));
        press(&mut app, ctrl(Key::Char('f')));
        until(&mut app, App::browser_ready);
        key(&mut app, Key::Home);
        press(
            &mut app,
            KeyPress {
                shift,
                ..ctrl(Key::Enter)
            },
        );
        let expected = if shift {
            music.clone()
        } else {
            music.join("a")
        };
        assert!(!app.view.notice_error, "{}", app.view.notice);
        assert!(
            matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if matches!(&dialog.purpose, TextPurpose::CreateFromFolder { folder, group_subfolders: true } if folder == &expected))
        );
    }
}

#[test]
fn folder_playlist_browser_marks_and_groups_without_dispatching_global_actions() {
    let directory = tempfile::tempdir().unwrap();
    let music = music_folders(directory.path());
    let mut app = App::new(directory.path()).unwrap();
    open_folder_picker(&mut app, &music);
    let Some(Dialog::Browser(browser)) = &mut app.view.dialog else {
        unreachable!()
    };
    browser.folder_playlists = Some(false);
    browser.playlist_name = None;
    key(&mut app, Key::Char(' '));
    assert_eq!(active_browser(&app).marked.len(), 1);
    key(&mut app, Key::Down);
    key(&mut app, Key::Insert);
    assert_eq!(active_browser(&app).marked.len(), 2);
    press(&mut app, ctrl(Key::Char('d')));
    assert!(active_browser(&app).marked.is_empty());
    press(&mut app, ctrl(Key::Char('a')));
    assert_eq!(active_browser(&app).marked.len(), 2);
    press(&mut app, ctrl(Key::Char('g')));
    assert_eq!(active_browser(&app).folder_playlists, Some(true));
    press(&mut app, ctrl(Key::Char(' ')));
    assert_eq!(active_browser(&app).marked.len(), 1);
    key(&mut app, Key::Enter);
    until(&mut app, App::browser_ready);
    key(&mut app, Key::Backspace);
    until(&mut app, App::browser_ready);
    assert_eq!(active_browser(&app).marked.len(), 1);
}

#[test]
fn composition_keys_change_source_order_and_submit_the_selected_sources() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, ids) = seeded(directory.path());
    assert_eq!(ids.len(), 3);
    app.action(Action::ComposePlaylists).unwrap();
    assert!(
        matches!(&app.view.dialog, Some(Dialog::ComposePlaylists { ids: selected, .. }) if selected == &vec![ids[0]])
    );
    for _ in 1..3 {
        key(&mut app, Key::Down);
        key(&mut app, Key::Char(' '));
    }
    press(&mut app, ctrl(Key::Up));
    assert!(
        matches!(&app.view.dialog, Some(Dialog::ComposePlaylists { ids: selected, .. }) if selected == &vec![ids[0], ids[2], ids[1]])
    );
    press(&mut app, ctrl(Key::Down));
    press(&mut app, ctrl(Key::Home));
    press(&mut app, ctrl(Key::Down));
    assert!(
        matches!(&app.view.dialog, Some(Dialog::ComposePlaylists { ids: selected, .. }) if selected == &vec![ids[1], ids[0], ids[2]])
    );
    key(&mut app, Key::End);
    key(&mut app, Key::Char(' '));
    key(&mut app, Key::Enter);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if matches!(&dialog.purpose, TextPurpose::ComposePlaylist(selected) if selected == &vec![ids[1], ids[0]]))
    );
}

#[test]
fn folder_management_keys_edit_remove_and_toggle_scan_options() {
    let directory = tempfile::tempdir().unwrap();
    let music = music_folders(directory.path());
    let (mut app, ids) = seeded(directory.path());
    app.store
        .set_playlist_folder(ids[0], None, &music.join("a"), false)
        .unwrap();
    app.store
        .set_playlist_folder(ids[0], None, &music.join("b"), false)
        .unwrap();
    until(&mut app, |app| !app.busy());
    app.library.selected_playlist = Some(ids[0]);
    app.action(Action::PlaylistFolders).unwrap();
    key(&mut app, Key::Down);
    key(&mut app, Key::Enter);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if matches!(&dialog.purpose, TextPurpose::PlaylistFolder(id, Some(previous)) if *id == ids[0] && previous == &music.join("b").to_string_lossy()))
    );
    key(&mut app, Key::Escape);
    app.action(Action::PlaylistFolders).unwrap();
    key(&mut app, Key::Insert);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if matches!(dialog.purpose, TextPurpose::PlaylistFolder(id, None) if id == ids[0]))
    );
    key(&mut app, Key::Escape);
    app.action(Action::PlaylistFolders).unwrap();
    key(&mut app, Key::Delete);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Text(dialog)) if matches!(&dialog.purpose, TextPurpose::RemovePlaylistFolder(id, _) if *id == ids[0]))
    );
    key(&mut app, Key::Escape);
    app.action(Action::PlaylistFolders).unwrap();
    let scans_subfolders = app.settings.scan_subfolders;
    key(&mut app, Key::Char(' '));
    assert_eq!(app.settings.scan_subfolders, !scans_subfolders);
    assert!(matches!(app.view.dialog, Some(Dialog::Folders { .. })));
}

#[test]
fn panel_tree_keys_expand_categories_and_toggle_visibility_with_modifier_guards() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    let category = panel_rows(&[])
        .iter()
        .position(|row| matches!(row, PanelRow::Category(Panel::Application)))
        .unwrap();
    app.view.dialog = Some(Dialog::Panels {
        selected: category,
        expanded: Vec::new(),
    });
    key(&mut app, Key::Right);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Panels { expanded, .. }) if expanded.contains(&Panel::Application))
    );
    key(&mut app, Key::Down);
    let Some(Dialog::Panels { selected, expanded }) = &app.view.dialog else {
        unreachable!()
    };
    let PanelRow::Control(control) = panel_rows(expanded)[*selected] else {
        panic!("Expected a command row")
    };
    press(&mut app, ctrl(Key::Right));
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Panels { selected, .. }) if *selected == category + 1)
    );
    key(&mut app, Key::Char(' '));
    assert!(app.settings.workspace.hidden_controls.contains(&control));
    key(&mut app, Key::Left);
    assert!(
        matches!(&app.view.dialog, Some(Dialog::Panels { selected, expanded }) if *selected == category && !expanded.contains(&Panel::Application))
    );
    key(&mut app, Key::Char(' '));
    assert!(app.settings.workspace.hidden.contains(&Panel::Application));
    assert!(matches!(app.view.dialog, Some(Dialog::Panels { .. })));
}

#[test]
fn playlist_confirmation_supports_navigation_and_russian_yes_no_keys() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, ids) = seeded(directory.path());
    app.library.selected_playlist = Some(ids[0]);
    app.view.focus = Focus::Playlists;
    app.action(Action::Delete).unwrap();
    key(&mut app, Key::Left);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::RemovePlaylist { confirm: true, .. })
    ));
    key(&mut app, Key::Right);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::RemovePlaylist { confirm: false, .. })
    ));
    key(&mut app, Key::Char('н'));
    assert!(app.view.dialog.is_none());
    assert!(app.playlists().iter().any(|playlist| playlist.id == ids[0]));
    app.action(Action::Delete).unwrap();
    key(&mut app, Key::Up);
    key(&mut app, Key::Tab);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::RemovePlaylist { confirm: false, .. })
    ));
    key(&mut app, Key::Char('д'));
    until(&mut app, |app| !app.busy());
    assert!(app.view.dialog.is_none());
    assert!(!app.playlists().iter().any(|playlist| playlist.id == ids[0]));
    assert!(
        ids[1..]
            .iter()
            .all(|id| app.playlists().iter().any(|playlist| playlist.id == *id))
    );
}
