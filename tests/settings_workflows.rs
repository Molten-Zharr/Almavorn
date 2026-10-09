use almavorn::{
    app::{App, Dialog, SettingsFocus, SettingsPage, Target},
    input::{Action, Input, Key, KeyPress, ShortcutGroup},
    model::{Language, RepeatMode},
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
        assert!(Instant::now() < deadline, "Timed out: {}", app.view.notice);
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
    assert!(!app.view.notice_error, "{}", app.view.notice);
}
fn select_row(app: &mut App, row: usize) {
    while app.settings_focus() != SettingsFocus::Parameters {
        key(app, Key::Tab);
    }
    key(app, Key::Home);
    for _ in 0..row {
        key(app, Key::Down);
    }
}
fn click(app: &mut App, predicate: impl Fn(&Target) -> bool) {
    mouse_click(app, predicate, false);
}
fn secondary_click(app: &mut App, predicate: impl Fn(&Target) -> bool) {
    mouse_click(app, predicate, true);
}
fn mouse_click(app: &mut App, predicate: impl Fn(&Target) -> bool, secondary: bool) {
    let mut terminal = Terminal::new(TestBackend::new(120, 50)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    let area = app
        .view
        .hits
        .iter()
        .find(|hit| hit.enabled && predicate(&hit.target))
        .unwrap()
        .area;
    app.handle(if secondary {
        Input::SecondaryClick {
            x: area.x,
            y: area.y,
        }
    } else {
        Input::Click {
            x: area.x,
            y: area.y,
            double: false,
        }
    });
    if !secondary {
        app.handle(Input::Release {
            x: area.x,
            y: area.y,
        });
    }
}
fn click_row(app: &mut App, row: usize) {
    select_row(app, row);
    click(
        app,
        |target| matches!(target,Target::Setting(index) if *index == row),
    );
}
fn shortcut_row(app: &App, action: Action) -> usize {
    let mut bindings: Vec<_> = app.settings.bindings.iter().collect();
    bindings.sort_by_key(|binding| binding.action.shortcut_group());
    1 + bindings
        .iter()
        .position(|binding| binding.action == action)
        .unwrap()
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

fn assert_theme_indicator(app: &mut App, expected: &str) {
    let terminal = rendered(app, 120, 50);
    let area = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingSelect(0)))
        .expect("theme selector must be visible")
        .area;
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    assert!(text.contains(expected), "Expected theme {expected}: {text}");
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
    assert_eq!(app.settings.appearance.font, FontFace::FiraCode);
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
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(97, 82, 80));
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.fg == Color::Rgb(255, 0, 0))
    );
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
    assert!(matches!(app.view.dialog, Some(Dialog::SettingsHelp { .. })));
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
                    app.view
                        .hits
                        .iter()
                        .any(|hit| hit.enabled && matches!(hit.target, Target::CloseDialog)),
                    "{page:?} {width}x{height}"
                );
                assert!(
                    app.view
                        .hits
                        .iter()
                        .all(|hit| hit.area.right() <= width && hit.area.bottom() <= height)
                );
                if width >= 70 {
                    let text = screen_text(&terminal);
                    assert!(
                        text.contains("Tab:") && text.contains("Enter") && text.contains("Esc"),
                        "{page:?}: {text}"
                    );
                }
                if page == SettingsPage::Palettes && width == 120 {
                    let text = screen_text(&terminal);
                    assert!(
                        text.contains("#F8F8F2") && text.contains("#F9F9FE"),
                        "{text}"
                    );
                }
                assert!(!app.view.hits.iter().any(|hit| matches!(
                    hit.target,
                    Target::Playlist(_) | Target::Track(_) | Target::Mode(_)
                )));
            }
        }
    }
}

#[test]
fn mouse_wheel_moves_one_settings_item_at_a_time() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    rendered(&mut app, 120, 50);
    let menu = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingsPage(SettingsPage::Themes)))
        .unwrap()
        .area;
    app.handle(Input::Scroll {
        x: menu.x,
        y: menu.y,
        delta: 3,
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    app.handle(Input::Scroll {
        x: menu.x,
        y: menu.y,
        delta: -3,
    });
    assert_eq!(app.settings_page(), SettingsPage::General);
    select_row(&mut app, 0);
    rendered(&mut app, 120, 50);
    let row = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingSelect(1)))
        .unwrap()
        .area;
    app.handle(Input::Scroll {
        x: row.x,
        y: row.y,
        delta: 3,
    });
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Settings { selected: 1 })
    ));
}

#[test]
fn theme_subtabs_support_mouse_keyboard_wheel_and_breadcrumbs() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    let baseline = rendered(&mut app, 120, 50);
    assert_eq!(
        app.view
            .hits
            .iter()
            .filter(|hit| hit.area.y > 0 && matches!(hit.target, Target::SettingsPage(_)))
            .count(),
        4
    );
    assert!(!app.view.hits.iter().any(|hit| matches!(
        hit.target,
        Target::SettingsPage(SettingsPage::Palettes | SettingsPage::Typography)
    )));
    assert_eq!(
        app.view
            .hits
            .iter()
            .filter(|hit| hit.area.y > 0 && matches!(hit.target, Target::SettingsTab(_)))
            .count(),
        3
    );
    let tab = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingsTab(SettingsPage::Palettes)))
        .unwrap()
        .area;
    let breadcrumb = app
        .view
        .hits
        .iter()
        .find(|hit| {
            hit.area.y == 0 && matches!(hit.target, Target::SettingsPage(SettingsPage::Themes))
        })
        .unwrap()
        .area;
    for area in [tab, breadcrumb] {
        app.handle(Input::Move {
            x: area.x,
            y: area.y,
        });
        let hovered = rendered(&mut app, 120, 50);
        assert_ne!(
            hovered.backend().buffer()[(area.x, area.y)].bg,
            baseline.backend().buffer()[(area.x, area.y)].bg
        );
        assert_eq!(app.settings_page(), SettingsPage::Palettes);
    }
    app.open_settings_page(SettingsPage::Themes);
    key(&mut app, Key::Tab);
    assert_eq!(app.settings_focus(), SettingsFocus::Subtabs);
    key(&mut app, Key::Right);
    assert_eq!(app.settings_page(), SettingsPage::Palettes);
    key(&mut app, Key::Tab);
    assert_eq!(app.settings_focus(), SettingsFocus::Parameters);
    app.handle(Input::Key(KeyPress {
        key: Key::Tab,
        ctrl: false,
        alt: false,
        shift: true,
    }));
    assert_eq!(app.settings_focus(), SettingsFocus::Subtabs);
    key(&mut app, Key::Left);
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    rendered(&mut app, 120, 50);
    app.handle(Input::Scroll {
        x: tab.x,
        y: tab.y,
        delta: 3,
    });
    assert_eq!(app.settings_page(), SettingsPage::Palettes);
    assert_eq!(app.settings_focus(), SettingsFocus::Subtabs);
    app.handle(Input::Scroll {
        x: tab.x,
        y: tab.y,
        delta: 3,
    });
    assert_eq!(app.settings_page(), SettingsPage::Typography);
    secondary_click(&mut app, |target| {
        matches!(target, Target::SettingsTab(SettingsPage::Typography))
    });
    assert_eq!(app.settings_page(), SettingsPage::Palettes);
    let terminal = rendered(&mut app, 120, 50);
    let old_tab = app
        .view
        .hits
        .iter()
        .find(|hit| {
            hit.area.y > 0 && matches!(hit.target, Target::SettingsTab(SettingsPage::Typography))
        })
        .unwrap()
        .area;
    let [r, g, b] = app.settings.current_palette().color("background");
    assert_eq!(
        terminal.backend().buffer()[(old_tab.x, old_tab.y)].bg,
        Color::Rgb(r, g, b)
    );
    click(&mut app, |target| {
        matches!(target, Target::SettingsTab(SettingsPage::Typography))
    });
    assert_eq!(app.settings_page(), SettingsPage::Typography);
    app.handle(Input::Scroll {
        x: breadcrumb.x,
        y: breadcrumb.y,
        delta: -3,
    });
    assert_eq!(app.settings_page(), SettingsPage::Typography);
    click(&mut app, |target| {
        matches!(target, Target::SettingsPage(SettingsPage::Themes))
    });
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    secondary_click(&mut app, |target| {
        matches!(target, Target::SettingsPage(SettingsPage::Themes))
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    assert!(!app.hovered(breadcrumb));
    app.open_settings_page(SettingsPage::Typography);
    click(&mut app, |target| {
        matches!(target, Target::SettingsPage(SettingsPage::Profiles))
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    click(&mut app, |target| {
        matches!(target, Target::SettingsPage(SettingsPage::General))
    });
    assert_eq!(app.settings_page(), SettingsPage::General);
    click(&mut app, |target| matches!(target, Target::CloseDialog));
    assert!(app.view.dialog.is_none());
}

#[test]
fn primary_and_secondary_clicks_change_settings_in_opposite_directions() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    let placement = app.settings.playlist_placement;
    click(&mut app, |target| {
        matches!(target, Target::SettingSelect(3))
    });
    assert_ne!(app.settings.playlist_placement, placement);
    secondary_click(&mut app, |target| {
        matches!(target, Target::SettingSelect(3))
    });
    assert_eq!(app.settings.playlist_placement, placement);
    app.open_settings_page(SettingsPage::Themes);
    click(&mut app, |target| {
        matches!(target, Target::SettingSelect(0))
    });
    assert_eq!(app.settings.current_palette().id, "classic-amber");
    secondary_click(&mut app, |target| matches!(target, Target::Setting(0)));
    assert_eq!(app.settings.current_palette().id, BRAND_PALETTE);
    app.open_settings_page(SettingsPage::Typography);
    let size = app.settings.appearance.font_size;
    click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(1, 1))
    });
    assert_eq!(app.settings.appearance.font_size, size + 1);
    secondary_click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(1, 1))
    });
    assert_eq!(app.settings.appearance.font_size, size);
    for _ in 0..40 {
        secondary_click(&mut app, |target| {
            matches!(target, Target::SettingSelect(1))
        });
    }
    assert_eq!(app.settings.appearance.font_size, 10);
    select_row(&mut app, 1);
    key(&mut app, Key::Enter);
    submit(&mut app, "23");
    assert_eq!(app.settings.appearance.font_size, 23);
    saved(&mut app);
}

#[test]
fn volume_scale_sets_full_range_and_saves_the_last_dragged_value() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    rendered(&mut app, 120, 50);
    let scale = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::SettingsVolume(_, area) => Some(area),
            _ => None,
        })
        .unwrap();
    assert!((8..=24).contains(&scale.width) && scale.height == 1);
    for (x, expected) in [(scale.x, 0.0), (scale.right() - 1, 1.0)] {
        app.handle(Input::Click {
            x,
            y: scale.y,
            double: false,
        });
        assert_eq!(app.settings.volume, expected);
        app.handle(Input::Release { x, y: scale.y });
        assert!(app.view.workspace.gesture.is_none());
        let terminal = rendered(&mut app, 120, 50);
        assert!(screen_text(&terminal).contains(&format!("{}%", (expected * 100.0) as u32)));
    }
    secondary_click(&mut app, |target| {
        matches!(target, Target::SettingsVolume(..))
    });
    assert!((app.settings.volume - 0.95).abs() < 0.001);
    click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(1, 1))
    });
    assert_eq!(app.settings.volume, 1.0);
    app.handle(Input::Click {
        x: scale.x,
        y: scale.y,
        double: false,
    });
    app.handle(Input::Move { x: 1, y: 5 });
    assert_eq!(app.settings_page(), SettingsPage::General);
    for offset in [0, scale.width / 4, scale.width / 2] {
        app.handle(Input::Drag {
            x: scale.x + offset,
            y: scale.bottom() + 1,
        });
    }
    app.handle(Input::Release {
        x: scale.x + scale.width / 2,
        y: scale.bottom() + 1,
    });
    assert!(app.view.workspace.gesture.is_none());
    let expected = f32::from(scale.width / 2) / f32::from(scale.width - 1);
    assert_eq!(app.settings.volume, expected);
    let terminal = rendered(&mut app, 120, 50);
    let percentage = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingAdjust(1, 1)))
        .unwrap()
        .area;
    assert_eq!(percentage.y, scale.y);
    assert!(screen_text(&terminal).contains(&format!("{}%", (expected * 100.0).round() as u32)));
    saved(&mut app);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.volume, expected);
}

#[test]
fn settings_hover_selects_sections_without_changing_preferences() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    let original_settings = serde_json::to_value(&app.settings).unwrap();
    rendered(&mut app, 120, 50);
    let section = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingsPage(SettingsPage::Themes)))
        .unwrap()
        .area;
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    assert_eq!(app.settings_focus(), SettingsFocus::Menu);
    let terminal = rendered(&mut app, 120, 50);
    let [r, g, b] = app.settings.current_palette().color("selection_accent");
    assert_eq!(
        terminal.backend().buffer()[(section.x, section.y)].bg,
        Color::Rgb(r, g, b)
    );
    app.handle(Input::Move {
        x: u16::MAX,
        y: u16::MAX,
    });
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    assert_eq!(
        serde_json::to_value(&app.settings).unwrap(),
        original_settings
    );
    key(&mut app, Key::Escape);
    rendered(&mut app, 120, 50);
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    key(&mut app, Key::F(2));
    rendered(&mut app, 120, 50);
    assert_eq!(app.settings_page(), SettingsPage::General);
    assert!(!app.hovered(section));
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    assert_eq!(app.settings_page(), SettingsPage::Themes);
}

#[test]
fn settings_hover_wheel_and_keyboard_share_one_current_section() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    rendered(&mut app, 120, 50);
    let section = app
        .view
        .hits
        .iter()
        .find(|hit| {
            hit.area.y > 0 && matches!(hit.target, Target::SettingsPage(SettingsPage::Profiles))
        })
        .unwrap()
        .area;
    let original_settings = serde_json::to_value(&app.settings).unwrap();
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    rendered(&mut app, 120, 50);
    for page in [SettingsPage::Themes, SettingsPage::Shortcuts] {
        app.handle(Input::Scroll {
            x: section.x,
            y: section.y,
            delta: 1,
        });
        assert_eq!(app.settings_page(), page);
        for _ in 0..3 {
            let terminal = rendered(&mut app, 120, 50);
            assert_eq!(app.settings_page(), page);
            assert!(!app.hovered(section));
            let [r, g, b] = app.settings.current_palette().color("background");
            assert_eq!(
                terminal.backend().buffer()[(section.x, section.y)].bg,
                Color::Rgb(r, g, b)
            );
            let highlighted = app
                .view
                .hits
                .iter()
                .filter(|hit| {
                    hit.area.y > 0
                        && matches!(hit.target, Target::SettingsPage(_))
                        && terminal.backend().buffer()[(hit.area.x, hit.area.y)].bg
                            != Color::Rgb(r, g, b)
                })
                .count();
            assert_eq!(highlighted, 1);
        }
    }
    key(&mut app, Key::Up);
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    key(&mut app, Key::Down);
    assert_eq!(app.settings_page(), SettingsPage::Themes);
    rendered(&mut app, 120, 50);
    assert!(!app.hovered(section));
    app.handle(Input::Move {
        x: section.x,
        y: section.y,
    });
    assert_eq!(app.settings_page(), SettingsPage::Profiles);
    assert_eq!(
        serde_json::to_value(&app.settings).unwrap(),
        original_settings
    );
}

#[test]
fn settings_hover_highlights_rows_without_replacing_the_selected_parameter() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::General);
    select_row(&mut app, 3);
    rendered(&mut app, 120, 50);
    let placement = app.settings.playlist_placement;
    let volume = app.settings.volume;
    let value = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingAdjust(1, 1)))
        .unwrap()
        .area;
    app.handle(Input::Move {
        x: value.x,
        y: value.y,
    });
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Settings { selected: 3 })
    ));
    assert_eq!(app.settings.volume, volume);
    let before = rendered(&mut app, 120, 50);
    let row = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::SettingSelect(1)))
        .unwrap()
        .area;
    let [r, g, b] = app
        .settings
        .current_palette()
        .color("selected_file_background");
    assert_eq!(
        before.backend().buffer()[(row.x, row.y)].bg,
        Color::Rgb(r, g, b)
    );
    key(&mut app, Key::Right);
    assert_ne!(app.settings.playlist_placement, placement);
    assert_eq!(app.settings.volume, volume);
    assert!(!app.hovered(value));
    // Clicking a value deliberately changes the selection and its value once.
    click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(1, 1))
    });
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Settings { selected: 1 })
    ));
    assert!((app.settings.volume - volume - 0.05).abs() < 0.001);
    app.handle(Input::Scroll {
        x: value.x,
        y: value.y,
        delta: 1,
    });
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Settings { selected: 2 })
    ));
    for _ in 0..3 {
        let after = rendered(&mut app, 120, 50);
        let [r, g, b] = app.settings.current_palette().color("background");
        assert_eq!(
            after.backend().buffer()[(row.x, row.y)].bg,
            Color::Rgb(r, g, b)
        );
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::Settings { selected: 2 })
        ));
    }
}

#[test]
fn moving_across_parameters_to_the_edit_footer_keeps_its_target_and_activates_once() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Typography);
    select_row(&mut app, 1); // Exact font-size editor.
    let original = app.settings.appearance.clone();
    for index in [0, 2, 3] {
        rendered(&mut app, 120, 50);
        let row = app
            .view
            .hits
            .iter()
            .find(|hit| matches!(hit.target, Target::SettingSelect(value) if value == index))
            .unwrap()
            .area;
        app.handle(Input::Move { x: row.x, y: row.y });
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::Settings { selected: 1 })
        ));
        assert_eq!(
            serde_json::to_value(&app.settings.appearance).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
    }
    rendered(&mut app, 120, 50);
    let footer = app
        .view
        .hits
        .iter()
        .rev()
        .find(|hit| hit.enabled && matches!(hit.target, Target::Setting(1)))
        .unwrap()
        .area;
    app.handle(Input::Move {
        x: footer.x,
        y: footer.y,
    });
    rendered(&mut app, 120, 50);
    app.handle(Input::Click {
        x: footer.x,
        y: footer.y,
        double: false,
    });
    app.handle(Input::Release {
        x: footer.x,
        y: footer.y,
    });
    assert!(matches!(app.view.dialog, Some(Dialog::Text(ref dialog))
        if matches!(dialog.purpose, almavorn::app::TextPurpose::Settings(almavorn::app::SettingsEdit::FontSize))));
    assert_eq!(
        serde_json::to_value(&app.settings.appearance).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    submit(&mut app, "21");
    assert_eq!(app.settings.appearance.font_size, 21);
    assert_eq!(app.settings.appearance.font, original.font);
    assert_eq!(app.settings.appearance.borders, original.borders);
    assert_eq!(app.settings.appearance.corners, original.corners);
}

#[test]
fn playback_modes_save_in_the_active_profile_and_survive_restart() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let defaults = app.settings.profile_preferences();
    app.action(Action::CycleRepeat).unwrap();
    app.action(Action::ToggleShuffle).unwrap();
    saved(&mut app);
    let active = app
        .settings
        .profiles
        .iter()
        .find(|profile| profile.id == app.settings.active_profile)
        .unwrap();
    assert_eq!(active.preferences.repeat, RepeatMode::Playlist);
    assert!(active.preferences.shuffle);
    let looping = app.settings.profile_preferences();
    app.settings.apply_preferences(defaults);
    assert_eq!(app.settings.repeat, RepeatMode::Off);
    assert!(!app.settings.shuffle);
    app.settings.apply_preferences(looping);
    assert_eq!(app.settings.repeat, RepeatMode::Playlist);
    assert!(app.settings.shuffle);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.repeat, RepeatMode::Playlist);
    assert!(app.settings.shuffle);
}

#[test]
fn old_settings_default_playback_modes_and_add_shortcuts_without_overwriting_custom_keys() {
    let directory = Directory::new();
    let mut store = Store::open(&directory.0).unwrap();
    let mut settings = store.settings().unwrap();
    settings
        .bindings
        .retain(|binding| !matches!(binding.action, Action::CycleRepeat | Action::ToggleShuffle));
    for (action, key) in [(Action::Quit, 'l'), (Action::Help, 'h')] {
        settings
            .bindings
            .iter_mut()
            .find(|binding| binding.action == action)
            .unwrap()
            .key = KeyPress::plain(Key::Char(key));
    }
    settings.sync_active_profile();
    let mut legacy = serde_json::to_value(settings).unwrap();
    legacy.as_object_mut().unwrap().remove("repeat");
    legacy.as_object_mut().unwrap().remove("shuffle");
    for profile in legacy["profiles"].as_array_mut().unwrap() {
        profile["preferences"]
            .as_object_mut()
            .unwrap()
            .remove("repeat");
        profile["preferences"]
            .as_object_mut()
            .unwrap()
            .remove("shuffle");
    }
    let settings = serde_json::from_value(legacy).unwrap();
    store.save_settings(&settings).unwrap();
    drop(store);
    let mut app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.repeat, RepeatMode::Off);
    assert!(!app.settings.shuffle);
    assert_eq!(app.settings.profiles[0].preferences.repeat, RepeatMode::Off);
    assert!(!app.settings.profiles[0].preferences.shuffle);
    for (action, key) in [(Action::Quit, 'l'), (Action::Help, 'h')] {
        assert_eq!(
            app.settings
                .bindings
                .iter()
                .find(|binding| binding.action == action)
                .unwrap()
                .key,
            KeyPress::plain(Key::Char(key))
        );
    }
    let repeat = app
        .settings
        .bindings
        .iter()
        .find(|binding| binding.action == Action::CycleRepeat)
        .unwrap()
        .key;
    let shuffle = app
        .settings
        .bindings
        .iter()
        .find(|binding| binding.action == Action::ToggleShuffle)
        .unwrap()
        .key;
    assert_ne!(repeat, shuffle);
    for key in [repeat, shuffle] {
        assert_eq!(
            app.settings
                .bindings
                .iter()
                .filter(|binding| binding.key == key)
                .count(),
            1
        );
    }
    app.handle(Input::Key(repeat));
    assert_eq!(app.settings.repeat, RepeatMode::Playlist);
    app.handle(Input::Key(shuffle));
    assert!(app.settings.shuffle);
}

#[test]
fn choosing_a_theme_applies_it_immediately_with_keyboard_and_mouse() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.current_palette().id, "classic-amber");
    let terminal = rendered(&mut app, 120, 50);
    assert_theme_indicator(&mut app, "Classic Amber");
    assert!(!screen_text(&terminal).contains("Применить пресет"));
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(18, 22, 29)
    );
    click(&mut app, |target| {
        matches!(target, Target::SettingAdjust(0, 1))
    });
    assert_eq!(app.settings.current_palette().id, "classic-violet");
    click_row(&mut app, 0);
    assert_eq!(app.settings.current_palette().id, "light");
    assert_eq!(app.settings.appearance.palette_ids[1], BRAND_PALETTE);
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    let terminal = rendered(&mut app, 120, 50);
    assert_eq!(app.settings.current_palette().id, "light");
    assert_theme_indicator(&mut app, "Light");
    assert_eq!(
        terminal.backend().buffer()[(0, 0)].bg,
        Color::Rgb(239, 242, 246)
    );
}

#[test]
fn theme_indicator_tracks_profiles_modes_and_custom_appearance() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::Insert);
    submit(&mut app, "Violet profile");
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    app.open_settings_page(SettingsPage::Profiles);
    select_row(&mut app, 0);
    key(&mut app, Key::Left);
    click_row(&mut app, 1);
    app.open_settings_page(SettingsPage::Themes);
    assert_eq!(app.settings.current_palette().id, "classic-amber");
    assert_theme_indicator(&mut app, "Classic Amber");
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.current_palette().id, "classic-violet");
    app.set_mode(almavorn::model::Mode::Chaos).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    assert_theme_indicator(&mut app, "Molten-Zharr");
    app.open_settings_page(SettingsPage::Typography);
    click_row(&mut app, 1);
    submit(&mut app, "23");
    app.open_settings_page(SettingsPage::Themes);
    assert_theme_indicator(&mut app, "Пользовательская");
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    assert_theme_indicator(&mut app, "Пользовательская");
}

#[test]
fn copied_theme_keeps_its_name_after_restart_and_deletion_keeps_appearance() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    key(&mut app, Key::Insert);
    submit(&mut app, "My theme");
    assert_theme_indicator(&mut app, "My theme");
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::Insert);
    submit(&mut app, "Another profile");
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    assert_theme_indicator(&mut app, "My theme");
    let before = serde_json::to_value(app.settings.current_style()).unwrap();
    key(&mut app, Key::F(3));
    submit(&mut app, "Renamed theme");
    assert_theme_indicator(&mut app, "Renamed theme");
    key(&mut app, Key::Delete);
    key(&mut app, Key::Enter);
    assert!(
        !app.settings
            .presets
            .iter()
            .any(|p| p.name == "Renamed theme")
    );
    assert_eq!(
        serde_json::to_value(app.settings.current_style()).unwrap(),
        before
    );
    app.settings.validate_catalogs().unwrap();
    saved(&mut app);
}

#[test]
fn theme_selection_is_inferred_for_settings_saved_before_preset_ids() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
    key(&mut app, Key::Left);
    saved(&mut app);
    let mut json = serde_json::to_value(app.store.settings().unwrap()).unwrap();
    drop(app);
    json["appearance"]
        .as_object_mut()
        .unwrap()
        .remove("preset_ids");
    for profile in json["profiles"].as_array_mut().unwrap() {
        profile["preferences"]["appearance"]
            .as_object_mut()
            .unwrap()
            .remove("preset_ids");
    }
    // Older configurations can have removed the bundled preset from their catalog.
    json["presets"]
        .as_array_mut()
        .unwrap()
        .retain(|preset| preset["id"] != BRAND_PALETTE);
    let connection = rusqlite::Connection::open(directory.0.join("almavorn.sqlite")).unwrap();
    connection
        .execute(
            "UPDATE settings SET value=?1 WHERE key='app'",
            [json.to_string()],
        )
        .unwrap();
    drop(connection);
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Themes);
    assert_theme_indicator(&mut app, "Light");
    assert_eq!(app.settings.current_palette().id, "light");
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.current_palette().id, "classic-amber");
    saved(&mut app);
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
    let row = shortcut_row(&app, Action::Settings);
    click_row(&mut app, row);
    let original = app.settings.bindings[index].key;
    click(&mut app, |target| {
        matches!(target, Target::BindingKey(Key::F(1)))
    });
    assert!(app.view.notice_error);
    assert_eq!(app.settings.bindings[index].key, original);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::CaptureBinding { .. })
    ));
    click(&mut app, |target| {
        matches!(target, Target::BindingKey(Key::Up))
    });
    assert!(app.view.notice_error);
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
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
    key(&mut app, Key::Escape);
    app.handle(Input::Key(app.settings.bindings[index].key));
    assert_eq!(app.settings_page(), SettingsPage::General);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
    saved(&mut app);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.bindings[index].key.label(), "Ctrl+Alt+J");
}

#[test]
fn shortcut_dividers_skip_navigation_and_keep_original_binding_indices() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.bindings.reverse();
    let original = serde_json::to_value(&app.settings.bindings).unwrap();
    let indices: Vec<_> = ShortcutGroup::ALL
        .into_iter()
        .flat_map(|group| {
            app.settings
                .bindings
                .iter()
                .enumerate()
                .filter(move |(_, binding)| binding.action.shortcut_group() == group)
                .map(|(index, _)| index)
        })
        .collect();

    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        app.open_settings_page(SettingsPage::Shortcuts);
        let terminal = rendered(&mut app, 140, 50);
        let text = screen_text(&terminal);
        for group in ShortcutGroup::ALL {
            assert!(text.contains(group.name(language)));
        }
    }

    for (width, height) in [(120, 50), (70, 30), (32, 18)] {
        app.open_settings_page(SettingsPage::Shortcuts);
        key(&mut app, Key::Right);
        for (position, &index) in indices.iter().enumerate() {
            key(&mut app, Key::Down);
            let row = position + 1;
            assert!(
                matches!(app.view.dialog, Some(Dialog::Settings { selected }) if selected == row)
            );
            rendered(&mut app, width, height);
            let area = app
                .view
                .hits
                .iter()
                .find(|hit| matches!(hit.target, Target::Setting(selected) if selected == row))
                .expect("selected binding stays visible while scrolling")
                .area;
            assert!(area.right() <= width && area.bottom() <= height);
            app.handle(Input::Click {
                x: area.x,
                y: area.y,
                double: false,
            });
            assert!(
                matches!(app.view.dialog, Some(Dialog::CaptureBinding { index: selected }) if selected == index)
            );
            key(&mut app, Key::Escape);
        }
        key(&mut app, Key::Home);
        key(&mut app, Key::Down);
        rendered(&mut app, width, height);
        let row = app
            .view
            .hits
            .iter()
            .find(|hit| matches!(hit.target, Target::SettingSelect(1)))
            .unwrap()
            .area;
        let divider = (row.x, row.y - 2);
        assert!(
            !app.view
                .hits
                .iter()
                .any(|hit| hit.area.contains(divider.into()))
        );
        app.handle(Input::Click {
            x: divider.0,
            y: divider.1,
            double: false,
        });
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::Settings { selected: 1 })
        ));
        app.handle(Input::Scroll {
            x: divider.0,
            y: divider.1,
            delta: 1,
        });
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::Settings { selected: 2 })
        ));
    }
    assert_eq!(
        serde_json::to_value(&app.settings.bindings).unwrap(),
        original
    );
}

#[test]
fn settings_editors_keep_context_and_mouse_return_when_the_window_shrinks() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    let mut dialogs = Vec::new();
    app.open_settings_page(SettingsPage::Profiles);
    key(&mut app, Key::Insert);
    dialogs.push(app.view.dialog.clone().unwrap());
    submit(&mut app, "Second profile");
    key(&mut app, Key::Delete);
    dialogs.push(app.view.dialog.clone().unwrap());
    key(&mut app, Key::Escape);
    key(&mut app, Key::F(1));
    dialogs.push(app.view.dialog.clone().unwrap());
    key(&mut app, Key::Escape);
    app.open_settings_page(SettingsPage::Shortcuts);
    click_row(&mut app, 1);
    dialogs.push(app.view.dialog.clone().unwrap());
    for dialog in dialogs {
        for (width, height) in [(120, 50), (70, 30), (44, 22), (20, 8)] {
            app.view.dialog = Some(dialog.clone());
            rendered(&mut app, width, height);
            assert!(
                app.view
                    .hits
                    .iter()
                    .all(|hit| hit.area.right() <= width && hit.area.bottom() <= height)
            );
            assert!(
                !app.view
                    .hits
                    .iter()
                    .any(|hit| matches!(hit.target, Target::SettingsPage(_)))
            );
            let close = app
                .view
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
            assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
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
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::ConfirmSettings { .. })
    ));
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
    assert!(app.view.notice_error);
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
    until(&mut app, |app| app.view.notice_error);
    assert_eq!(app.settings.palettes, palettes);
    assert_eq!(fs::read(&path).unwrap(), b"keep this file");
    ctrl(&mut app, 'e');
    submit(&mut app, path.to_str().unwrap());
    until(&mut app, |app| app.view.notice_error);
    assert_eq!(fs::read(&path).unwrap(), b"keep this file");
    assert_eq!(app.settings.palettes, palettes);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
}

#[test]
fn palette_picker_synchronizes_controls_and_only_saves_on_apply() {
    fn color(app: &App) -> &almavorn::color_picker::ColorPicker {
        let Some(Dialog::Text(dialog)) = &app.view.dialog else {
            panic!("Expected color editor");
        };
        dialog.color_picker.as_ref().unwrap()
    }
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Palettes);
    let palettes = app.settings.palettes.clone();
    rendered(&mut app, 120, 50);
    let swatch = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::Setting(3)) && hit.area.width == 1)
        .unwrap()
        .area;
    app.handle(Input::Click {
        x: swatch.x,
        y: swatch.y,
        double: false,
    });
    app.handle(Input::Release {
        x: swatch.x,
        y: swatch.y,
    });
    assert_eq!(color(&app).original, [0, 0, 0]);
    ctrl(&mut app, 'a');
    app.handle(Input::Text("#102030".into()));
    assert_eq!(color(&app).hsv.rgb(), [16, 32, 48]);
    assert_eq!(color(&app).channel(3), 16.0);
    assert_eq!(color(&app).channel(4), 32.0);
    assert_eq!(color(&app).channel(5), 48.0);

    rendered(&mut app, 120, 50);
    let wheel = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::ColorWheel(area) => Some(area),
            _ => None,
        })
        .unwrap();
    let brightness = color(&app).hsv.value;
    app.handle(Input::Click {
        x: wheel.right() - 1,
        y: wheel.y + wheel.height / 2,
        double: false,
    });
    assert!(app.view.workspace.gesture.is_some());
    assert_eq!(color(&app).hsv.value, brightness);
    assert!(color(&app).hsv.saturation > 0.99);
    app.handle(Input::Release {
        x: u16::MAX,
        y: u16::MAX,
    });
    assert!(app.view.workspace.gesture.is_none());
    assert_eq!(app.settings.palettes, palettes);
    click(&mut app, |target| matches!(target, Target::ColorReset));
    assert_eq!(color(&app).hsv.rgb(), [0, 0, 0]);

    click(&mut app, |target| matches!(target, Target::ColorSelect(3)));
    key(&mut app, Key::Home);
    key(&mut app, Key::Right);
    assert_eq!(color(&app).channel(3), 1.0);
    app.handle(Input::Key(KeyPress {
        key: Key::Right,
        shift: true,
        ctrl: false,
        alt: false,
    }));
    assert_eq!(color(&app).channel(3), 11.0);
    key(&mut app, Key::End);
    assert_eq!(color(&app).channel(3), 255.0);
    let label = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::ColorSelect(3)))
        .unwrap()
        .area;
    app.handle(Input::Scroll {
        x: label.x,
        y: label.y,
        delta: 1,
    });
    assert_eq!(color(&app).channel(3), 254.0);
    assert_eq!(app.settings.palettes, palettes);
    click(&mut app, |target| matches!(target, Target::CloseDialog));
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
    assert_eq!(app.settings.palettes, palettes);

    click_row(&mut app, 3);
    ctrl(&mut app, 'a');
    app.handle(Input::Text("#102030".into()));
    ctrl(&mut app, 'a');
    app.handle(Input::Text("invalid".into()));
    assert_eq!(color(&app).hsv.rgb(), [16, 32, 48]);
    rendered(&mut app, 120, 50);
    assert!(
        app.view
            .hits
            .iter()
            .any(|hit| { matches!(hit.target, Target::Submit) && !hit.enabled })
    );
    key(&mut app, Key::Enter);
    assert!(app.view.notice_error);
    assert_eq!(color(&app).hsv.rgb(), [16, 32, 48]);
    assert_eq!(app.settings.palettes, palettes);
    ctrl(&mut app, 'a');
    app.handle(Input::Text("#123456".into()));
    click(&mut app, |target| matches!(target, Target::Submit));
    assert!(!app.view.notice_error, "{}", app.view.notice);
    assert_eq!(app.settings.palettes[0].color("background"), [18, 52, 86]);
    let mut expected = palettes;
    expected[0].colors.insert("background".into(), [18, 52, 86]);
    assert_eq!(app.settings.palettes, expected);
    saved(&mut app);
    drop(app);
    let app = App::new(&directory.0).unwrap();
    assert_eq!(app.settings.palettes, expected);
}

#[test]
fn palette_picker_keeps_controls_and_exit_accessible_at_small_sizes() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.language = Language::Russian;
    app.open_settings_page(SettingsPage::Palettes);
    click_row(&mut app, 3);
    for graphical in [false, true] {
        app.view.graphical_keycaps = graphical;
        for (width, height) in [(120, 50), (70, 30), (47, 26), (44, 22), (32, 18), (20, 8)] {
            for _ in 0..7 {
                let terminal = rendered(&mut app, width, height);
                let bounds = terminal.backend().buffer().area;
                for hit in &app.view.hits {
                    assert_eq!(hit.area.intersection(bounds), hit.area, "{width}×{height}");
                }
                assert!(
                    app.view
                        .hits
                        .iter()
                        .any(|hit| { hit.enabled && matches!(hit.target, Target::CloseDialog) })
                );
                if height >= 22 {
                    assert_eq!(
                        app.view
                            .hits
                            .iter()
                            .filter(|hit| matches!(hit.target, Target::ColorSlider(_, _)))
                            .count(),
                        6
                    );
                }
                if width >= 24 && height >= 18 {
                    assert!(
                        app.view
                            .hits
                            .iter()
                            .any(|hit| { hit.enabled && matches!(hit.target, Target::ColorHex) })
                    );
                    let Some(Dialog::Text(dialog)) = &app.view.dialog else {
                        panic!("Expected color editor");
                    };
                    if let Some(selected) = dialog.color_picker.as_ref().unwrap().selected {
                        assert!(app.view.hits.iter().any(|hit| {
                            matches!(hit.target, Target::ColorSlider(index, _) if index == selected)
                        }));
                    }
                }
                key(&mut app, Key::Tab);
            }
        }
    }
    key(&mut app, Key::Escape);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
}

#[test]
fn theme_presets_store_font_geometry_and_palette_with_full_crud() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.open_settings_page(SettingsPage::Typography);
    click_row(&mut app, 0);
    assert_eq!(app.settings.appearance.font, FontFace::FiraCodeBold);
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
    select_row(&mut app, 0);
    key(&mut app, Key::Right);
    assert_eq!(app.settings.appearance.font_size, 16);
    key(&mut app, Key::Left);
    assert_eq!(app.settings.appearance.font_size, 24);
    assert_eq!(app.settings.appearance.borders, BorderWeight::Double);
    click_row(&mut app, 5);
    saved(&mut app);
    drop(app);
    let mut app = App::new(&directory.0).unwrap();
    let preset = app.settings.presets.iter().find(|p| p.id == id).unwrap();
    assert_eq!(preset.name, "Comfort");
    assert_eq!(preset.style.font_size, 24);
    app.open_settings_page(SettingsPage::Themes);
    select_row(&mut app, 0);
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
    assert!(matches!(app.view.dialog, Some(Dialog::SettingsHelp { .. })));
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
    assert!(app.view.notice_error);
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
    let connection = rusqlite::Connection::open(directory.0.join("almavorn.sqlite")).unwrap();
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
