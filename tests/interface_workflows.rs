use almavorn::{
    app::{App, CommandMenu, Dialog, Focus, SettingsPage, Target},
    input::{Action, Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode, Placement},
    preferences::{BorderWeight, Corners, FontFace},
    ui,
    workspace::{Axis, Control, Dock, Panel, WorkspaceLayout},
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
            "almavorn-interface-{}-{}",
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
}
fn key(app: &mut App, key: Key) {
    app.handle(Input::Key(KeyPress::plain(key)));
}
fn ctrl_b(app: &mut App) {
    app.handle(Input::Key(KeyPress {
        key: Key::Char('b'),
        ctrl: true,
        alt: false,
        shift: false,
    }));
}
fn wait(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.busy() {
        assert!(Instant::now() < deadline);
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn shift_tab(app: &mut App) {
    app.handle(Input::Key(KeyPress {
        key: Key::Tab,
        ctrl: false,
        alt: false,
        shift: true,
    }));
}

fn toolbar_button(app: &mut App, predicate: impl Fn(&Target) -> bool) {
    for _ in 0..app.toolbar_items().len() {
        let index = app
            .view
            .toolbar_selected
            .expect("Command bar has keyboard focus");
        if predicate(&app.toolbar_items()[index].target) {
            return;
        }
        key(app, Key::Right);
    }
    panic!("Command bar button is unreachable");
}

#[test]
fn command_menus_skip_unavailable_rows_and_stop_at_the_last_enabled_action() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    click(&mut app, |target| {
        matches!(target, Target::CommandMenu(CommandMenu::Playlist))
    });
    let items = app.command_items(CommandMenu::Playlist);
    assert_eq!(items.iter().filter(|item| item.enabled).count(), 1);
    for key_value in [Key::Down, Key::PageDown, Key::Up, Key::PageUp] {
        for _ in 0..10 {
            key(&mut app, key_value);
        }
        assert!(matches!(
            app.view.dialog,
            Some(Dialog::Commands { selected: 0, .. })
        ));
    }
    key(&mut app, Key::Escape);
    click(&mut app, |target| {
        matches!(target, Target::CommandMenu(CommandMenu::Application))
    });
    let enabled: Vec<_> = app
        .command_items(CommandMenu::Application)
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.enabled.then_some(index))
        .collect();
    for &index in &enabled {
        assert!(
            matches!(app.view.dialog, Some(Dialog::Commands { selected, .. }) if selected == index)
        );
        key(&mut app, Key::Down);
    }
    for _ in 0..5 {
        key(&mut app, Key::Down);
    }
    assert!(
        matches!(app.view.dialog, Some(Dialog::Commands { selected, .. }) if selected == *enabled.last().unwrap())
    );
    for &index in enabled.iter().rev() {
        assert!(
            matches!(app.view.dialog, Some(Dialog::Commands { selected, .. }) if selected == index)
        );
        key(&mut app, Key::Up);
    }
    key(&mut app, Key::PageDown);
    let selected = match app.view.dialog {
        Some(Dialog::Commands { selected, .. }) => selected,
        _ => unreachable!(),
    };
    assert!(enabled.contains(&selected));
    key(&mut app, Key::Escape);
    // A completely disabled context menu stays inert under keys and the wheel.
    app.view.dialog = Some(Dialog::Commands {
        menu: CommandMenu::Player,
        selected: 0,
        anchor: ratatui::layout::Position::new(1, 1),
    });
    app.settings
        .workspace
        .hidden_controls
        .extend([Action::VolumeUp, Action::VolumeDown].map(Control::Action));
    render(&mut app, 120, 50);
    assert!(
        app.command_items(CommandMenu::Player)
            .iter()
            .all(|item| !item.enabled)
    );
    key(&mut app, Key::Down);
    app.handle(Input::Scroll {
        x: 2,
        y: 3,
        delta: 1,
    });
    key(&mut app, Key::Enter);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Commands { selected: 0, .. })
    ));
}

#[test]
fn help_stops_at_its_last_page_and_keeps_wrapped_content_visible_after_resizing() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        for (width, height) in [(140, 45), (70, 30), (32, 18)] {
            app.action(Action::Help).unwrap();
            render(&mut app, width, height);
            let mut reached_bottom = false;
            for _ in 0..100 {
                let before = match app.view.dialog {
                    Some(Dialog::Help { offset }) => offset,
                    _ => unreachable!(),
                };
                key(&mut app, Key::PageDown);
                let terminal = render(&mut app, width, height);
                let after = match app.view.dialog {
                    Some(Dialog::Help { offset }) => offset,
                    _ => unreachable!(),
                };
                if before == after {
                    let text: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|cell| cell.symbol())
                        .collect();
                    assert!(
                        text.contains(app.text("separately.", "отдельно.")),
                        "{width}x{height}: {text}"
                    );
                    reached_bottom = true;
                    break;
                }
            }
            assert!(reached_bottom);
            let before = match app.view.dialog {
                Some(Dialog::Help { offset }) => offset,
                _ => unreachable!(),
            };
            // No render is needed to clamp subsequent events to the measured limit.
            for _ in 0..20 {
                key(&mut app, Key::Down);
                app.handle(Input::Scroll {
                    x: 20,
                    y: 8,
                    delta: 1,
                });
            }
            assert!(matches!(app.view.dialog, Some(Dialog::Help { offset }) if offset == before));
            key(&mut app, Key::Up);
            assert!(
                matches!(app.view.dialog, Some(Dialog::Help { offset }) if offset == before - 1)
            );
            render(&mut app, 180, 70);
            let enlarged_offset = match app.view.dialog {
                Some(Dialog::Help { offset }) => offset,
                _ => unreachable!(),
            };
            assert!(enlarged_offset <= before);
            key(&mut app, Key::Escape);
        }
    }
    app.open_settings_page(SettingsPage::General);
    key(&mut app, Key::F(1));
    render(&mut app, 120, 50);
    for _ in 0..30 {
        key(&mut app, Key::Down);
    }
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::SettingsHelp { offset: 0, .. })
    ));
}

#[cfg(feature = "guitui")]
#[test]
fn desktop_keycaps_leave_blank_pixels_in_the_gaps_across_fonts_and_scroll_frames() {
    use ratatui::style::Color;
    use soft_ratatui::{EmbeddedTTF, SoftBackend, rusttype::Font};
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.view.graphical_keycaps = true;
    let mut checked = 0;
    for face in FontFace::ALL {
        for size in [10, 16, 32] {
            let bytes: &'static [u8] = match face {
                FontFace::FiraCode => include_bytes!("../assets/fonts/FiraCode-Regular.ttf"),
                FontFace::FiraCodeBold => include_bytes!("../assets/fonts/FiraCode-Bold.ttf"),
                FontFace::DejaVuMono => include_bytes!("../assets/fonts/DejaVuSansMono.ttf"),
            };
            let bold = if face == FontFace::DejaVuMono {
                None
            } else {
                Some(
                    Font::try_from_bytes(include_bytes!("../assets/fonts/FiraCode-Bold.ttf"))
                        .unwrap(),
                )
            };
            let backend = SoftBackend::<EmbeddedTTF>::new(
                110,
                42,
                size,
                Font::try_from_bytes(bytes).unwrap(),
                bold,
                None,
            );
            let mut terminal = Terminal::new(backend).unwrap();
            for palette in ["light", "classic-violet"] {
                app.settings.appearance.palette_ids = [palette.into(), palette.into()];
                let background = app.settings.current_palette().color("background");
                app.view.dialog = Some(Dialog::Help { offset: 0 });
                for offset in [0, 15, 0] {
                    if let Some(Dialog::Help { offset: current }) = &mut app.view.dialog {
                        *current = offset;
                    }
                    terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
                    let raster = terminal.backend();
                    for cap in &app.view.keycaps {
                        for gap in [
                            Rect::new(cap.area.right(), cap.area.y, 1, 2),
                            Rect::new(cap.area.x, cap.area.bottom(), cap.area.width + 1, 1),
                        ] {
                            if gap.intersection(raster.buffer.area) != gap
                                || (gap.y..gap.bottom()).any(|y| {
                                    (gap.x..gap.right()).any(|x| {
                                        let cell = &raster.buffer[(x, y)];
                                        cell.symbol() != " "
                                            || cell.bg
                                                != Color::Rgb(
                                                    background[0],
                                                    background[1],
                                                    background[2],
                                                )
                                    })
                                })
                            {
                                continue;
                            }
                            for y in usize::from(gap.y) * raster.char_height
                                ..usize::from(gap.bottom()) * raster.char_height
                            {
                                for x in usize::from(gap.x) * raster.char_width
                                    ..usize::from(gap.right()) * raster.char_width
                                {
                                    let pixel = (y * raster.get_pixmap_width() + x) * 3;
                                    assert_eq!(
                                        &raster.get_pixmap_data()[pixel..pixel + 3],
                                        &background,
                                        "{} {size}px {palette}, offset {offset}, key {}: stray pixel ({x}, {y})",
                                        face.name(),
                                        cap.label
                                    );
                                }
                            }
                            checked += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 100);
}

#[test]
fn command_bar_is_reachable_with_tab_and_every_visible_menu_opens_from_the_keyboard() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for (width, height) in [(120, 50), (32, 18)] {
        render(&mut app, width, height);
        app.library.query = "keep query".into();
        for _ in 0..=app.view.workspace.areas.len() {
            if app.view.toolbar_selected.is_some() {
                break;
            }
            key(&mut app, Key::Tab);
        }
        assert!(app.view.toolbar_selected.is_some());
        for menu in [
            CommandMenu::Add,
            CommandMenu::Playlist,
            CommandMenu::Application,
        ] {
            toolbar_button(
                &mut app,
                |target| matches!(target, Target::CommandMenu(value) if *value == menu),
            );
            let terminal = render(&mut app, width, height);
            let selected_area = app
                .view
                .hits
                .iter()
                .find(|hit| matches!(hit.target, Target::CommandMenu(value) if value == menu))
                .unwrap()
                .area;
            let [r, g, b] = app.settings.current_palette().color("selection_accent");
            assert_eq!(
                terminal.backend().buffer()[(selected_area.x, selected_area.y)].bg,
                ratatui::style::Color::Rgb(r, g, b)
            );
            key(&mut app, Key::Enter);
            assert!(
                matches!(app.view.dialog, Some(Dialog::Commands { menu: value, anchor, .. }) if value == menu && anchor == selected_area.as_position())
            );
            key(&mut app, Key::Down);
            key(&mut app, Key::Escape);
            assert!(app.view.dialog.is_none());
            assert!(app.view.toolbar_selected.is_some());
        }
        toolbar_button(&mut app, |target| {
            matches!(target, Target::Action(Action::Settings))
        });
        key(&mut app, Key::Enter);
        assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
        key(&mut app, Key::Escape);
        assert!(app.view.toolbar_selected.is_some());
        toolbar_button(&mut app, |target| {
            matches!(target, Target::Action(Action::Search))
        });
        key(&mut app, Key::Enter);
        assert!(matches!(app.view.dialog, Some(Dialog::Text(_))));
        key(&mut app, Key::Escape);
        key(&mut app, Key::Tab);
        assert!(app.view.toolbar_selected.is_none());
        assert_eq!(app.view.workspace.focus, app.view.workspace.areas[0].0);
        shift_tab(&mut app);
        assert!(app.view.toolbar_selected.is_some());
        shift_tab(&mut app);
        assert!(app.view.toolbar_selected.is_none());
        assert_eq!(
            app.view.workspace.focus,
            app.view.workspace.areas.last().unwrap().0
        );
        key(&mut app, Key::Tab);
        assert!(app.view.toolbar_selected.is_some());
        key(&mut app, Key::Escape);
        assert!(app.view.toolbar_selected.is_none());
        assert_eq!(app.library.query, "keep query");
    }
}

#[test]
fn keyboard_toolbar_skips_hidden_and_disabled_actions_and_supports_adjacent_menus() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings
        .workspace
        .hidden_controls
        .push(Control::Action(Action::Settings));
    render(&mut app, 120, 50);
    while app.view.toolbar_selected.is_none() {
        key(&mut app, Key::Tab);
    }
    assert!(
        !app.toolbar_items()
            .iter()
            .any(|item| matches!(item.target, Target::Action(Action::Settings)))
    );
    let enabled = app
        .toolbar_items()
        .iter()
        .filter(|item| item.enabled)
        .count();
    let initial = app.view.toolbar_selected;
    for _ in 0..enabled {
        let index = app.view.toolbar_selected.unwrap();
        assert!(app.toolbar_items()[index].enabled);
        key(&mut app, Key::Right);
    }
    assert_eq!(app.view.toolbar_selected, initial);
    toolbar_button(&mut app, |target| {
        matches!(target, Target::CommandMenu(CommandMenu::Add))
    });
    key(&mut app, Key::Enter);
    key(&mut app, Key::Right);
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Commands {
            menu: CommandMenu::Playlist,
            ..
        })
    ));
    key(&mut app, Key::Tab);
    assert!(app.view.dialog.is_none() && app.view.toolbar_selected.is_none());
    shift_tab(&mut app);
    toolbar_button(&mut app, |target| {
        matches!(target, Target::Action(Action::Panels))
    });
    key(&mut app, Key::Enter);
    assert!(app.view.layout_editing);
    render(&mut app, 120, 50);
    key(&mut app, Key::Enter);
    assert!(!app.view.layout_editing);
    key(&mut app, Key::Escape);
    let track_area = app.view.tracks_area;
    app.handle(Input::Click {
        x: track_area.x,
        y: track_area.y,
        double: false,
    });
    assert!(app.view.toolbar_selected.is_none());
}

#[test]
fn settings_values_share_a_right_column_and_narrow_windows_keep_font_values_readable() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for page in SettingsPage::ALL {
        app.open_settings_page(page);
        render(&mut app, 140, 45);
        let values: Vec<_> = app
            .view
            .hits
            .iter()
            .filter_map(|hit| {
                if matches!(hit.target, Target::Setting(_)) && hit.area.y < 40 {
                    Some(hit.area)
                } else {
                    None
                }
            })
            .collect();
        assert!(!values.is_empty());
        assert!(values.iter().all(|area| area.right() == values[0].right()));
        assert!(values[0].right() >= 134);
        let arrows: Vec<_> = app
            .view
            .hits
            .iter()
            .filter_map(|hit| {
                if matches!(hit.target, Target::SettingAdjust(_, 1)) {
                    Some(hit.area)
                } else {
                    None
                }
            })
            .collect();
        assert!(arrows.iter().all(|area| area.right() == 138));
    }
    app.settings.language = Language::Russian;
    app.open_settings_page(SettingsPage::Typography);
    for width in [44, 32] {
        for font in FontFace::ALL {
            app.settings.appearance.font = font;
            let terminal = render(&mut app, width, 22);
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains(font.name()), "{text}");
            assert!(text.contains("Одинарная"), "{text}");
            assert!(
                app.view
                    .hits
                    .iter()
                    .all(|hit| hit.area.intersection(Rect::new(0, 0, width, 22)) == hit.area)
            );
        }
    }
}

#[test]
fn library_has_the_space_and_layout_controls_are_hidden_until_requested() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.player_placement = Placement::Bottom;
    app.settings.playlist_placement = Placement::Left;
    for language in [Language::Russian, Language::English] {
        app.settings.language = language;
        for (width, height) in [(32, 18), (80, 30), (140, 45)] {
            let terminal = render(&mut app, width, height);
            assert_eq!(
                app.view.workspace.areas.len(),
                3,
                "{:?}",
                terminal.backend().buffer()
            );
            assert!(
                app.view
                    .workspace
                    .areas
                    .iter()
                    .all(|(panel, _)| Panel::CONTENT.contains(panel))
            );
            assert!(app.view.hits.iter().all(|hit| !matches!(
                hit.target,
                Target::PanelMove(_)
                    | Target::PanelClose(_)
                    | Target::PanelCollapse(_)
                    | Target::PanelResize(_)
            )));
            assert!(app.view.tracks_area.height > 0 && app.view.playlist_area.height > 0);
            assert!(
                app.view
                    .hits
                    .iter()
                    .all(|hit| hit.area.intersection(Rect::new(0, 0, width, height)) == hit.area)
            );
            if width == 140 {
                assert!(app.view.tracks_area.height >= height * 3 / 4);
                let player = app
                    .view
                    .workspace
                    .areas
                    .iter()
                    .find(|(panel, _)| *panel == Panel::Player)
                    .unwrap()
                    .1;
                assert!(player.height <= 5);
            }
        }
    }
    app.library.query = "keep this query".into();
    ctrl_b(&mut app);
    render(&mut app, 120, 50);
    assert!(app.view.layout_editing);
    for panel in Panel::CONTENT {
        assert!(
            app.view
                .hits
                .iter()
                .any(|hit| matches!(hit.target, Target::PanelMove(value) if value == panel))
        );
    }
    assert!(
        app.view
            .hits
            .iter()
            .any(|hit| matches!(hit.target, Target::PanelResize(_)))
    );
    key(&mut app, Key::Escape);
    assert!(!app.view.layout_editing);
    assert_eq!(app.library.query, "keep this query");
    app.settings.appearance.borders = BorderWeight::None;
    let terminal = render(&mut app, 120, 50);
    let player = app
        .view
        .workspace
        .areas
        .iter()
        .find(|(panel, _)| *panel == Panel::Player)
        .unwrap()
        .1;
    assert_eq!(
        terminal.backend().buffer()[(player.x + 1, player.y)].symbol(),
        app.text("P", "П")
    );
}

#[test]
fn menu_actions_preserve_the_selected_object_and_outside_clicks_only_dismiss() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    app.action(Action::NewPlaylist).unwrap();
    app.handle(Input::Text("Music".into()));
    key(&mut app, Key::Enter);
    wait(&mut app);
    let id = app.library.selected_playlist.unwrap();
    app.store
        .add_tracks(
            id,
            &[ImportedTrack {
                path: directory.0.join("track.wav"),
                title: "One".into(),
                artist: "Artist".into(),
                album: String::new(),
                duration_ms: 1000,
                tags: "{}".into(),
            }],
            false,
        )
        .unwrap();
    app.set_mode(Mode::Chaos).unwrap();
    wait(&mut app);
    render(&mut app, 120, 50);
    let track = app
        .view
        .hits
        .iter()
        .find(|hit| matches!(hit.target, Target::Track(_)))
        .unwrap()
        .area;
    app.handle(Input::SecondaryClick {
        x: track.x + 5,
        y: track.y,
    });
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Commands {
            menu: CommandMenu::Tracks,
            ..
        })
    ));
    assert!(app.view.focus == Focus::Tracks);
    let index = app
        .command_items(CommandMenu::Tracks)
        .iter()
        .position(|item| matches!(item.target, Target::Action(Action::Rename)))
        .unwrap();
    click(
        &mut app,
        |target| matches!(target, Target::CommandRow(value) if *value == index),
    );
    assert!(matches!(app.view.dialog, Some(Dialog::Text(_))));
    key(&mut app, Key::Escape);
    click(&mut app, |target| {
        matches!(target, Target::CommandMenu(CommandMenu::Playlist))
    });
    assert!(app.view.focus == Focus::Playlists);
    let selected = app.library.selected_entry;
    render(&mut app, 120, 50);
    app.handle(Input::Click {
        x: 0,
        y: 49,
        double: false,
    });
    assert!(app.view.dialog.is_none());
    assert_eq!(app.library.selected_entry, selected);
    key(&mut app, Key::F(10));
    assert!(matches!(
        app.view.dialog,
        Some(Dialog::Commands {
            menu: CommandMenu::Playlist,
            ..
        })
    ));
    key(&mut app, Key::Enter);
    assert!(matches!(app.view.dialog, Some(Dialog::Text(_))));
}

#[test]
fn hidden_commands_and_order_protection_apply_inside_menus() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings
        .workspace
        .hidden_controls
        .push(Control::Action(Action::Delete));
    click(&mut app, |target| {
        matches!(target, Target::CommandMenu(CommandMenu::Playlist))
    });
    let items = app.command_items(CommandMenu::Playlist);
    assert!(
        !items
            .iter()
            .any(|item| matches!(item.target, Target::Action(Action::Delete)))
    );
    let rename = items
        .iter()
        .position(|item| matches!(item.target, Target::Action(Action::Rename)))
        .unwrap();
    assert!(!items[rename].enabled);
    if let Some(Dialog::Commands { selected, .. }) = &mut app.view.dialog {
        *selected = rename;
    }
    key(&mut app, Key::Enter);
    assert!(matches!(app.view.dialog, Some(Dialog::Commands { .. })));
    assert!(!app.view.notice_error);
    key(&mut app, Key::Escape);
    ctrl_b(&mut app);
    click(&mut app, |target| matches!(target, Target::ManagePanels));
    assert!(matches!(app.view.dialog, Some(Dialog::Panels { .. })));
}

#[test]
fn compact_volume_supports_the_full_range_and_dragging() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.player_placement = Placement::Bottom;
    render(&mut app, 120, 50);
    assert!(
        !app.view
            .hits
            .iter()
            .any(|hit| hit.enabled && matches!(hit.target, Target::Seek(_)))
    );
    let bar = app
        .view
        .hits
        .iter()
        .find_map(|hit| {
            if let Target::PlaybackVolume(area) = hit.target {
                Some(area)
            } else {
                None
            }
        })
        .unwrap();
    assert!(bar.width <= 16);
    app.handle(Input::Click {
        x: bar.x,
        y: bar.y,
        double: false,
    });
    assert_eq!(app.settings.volume, 0.0);
    app.handle(Input::Drag {
        x: bar.right() + 20,
        y: bar.y,
    });
    assert_eq!(app.settings.volume, 1.0);
    app.handle(Input::Release {
        x: bar.right() + 20,
        y: bar.y,
    });
    assert!(app.view.workspace.gesture.is_none());
}

#[test]
fn old_layouts_drop_command_blocks_and_content_layouts_survive_normalization() {
    let library = Dock::split(
        Axis::Horizontal,
        220,
        Dock::Panel(Panel::Playlists),
        Dock::Panel(Panel::Tracks),
    );
    let content = Dock::split(Axis::Vertical, 700, library, Dock::Panel(Panel::Player));
    let toolbar = Dock::split(
        Axis::Horizontal,
        300,
        Dock::Panel(Panel::Application),
        Dock::split(
            Axis::Horizontal,
            500,
            Dock::Panel(Panel::Add),
            Dock::Panel(Panel::PlaylistActions),
        ),
    );
    let mut layout = WorkspaceLayout {
        version: 2,
        root: Some(Dock::split(Axis::Vertical, 300, toolbar, content)),
        hidden: vec![Panel::Add],
        hidden_controls: vec![Control::Action(Action::Undo)],
        collapsed: vec![],
    };
    layout.normalize();
    assert_eq!(layout.version, 3);
    let root = layout.root.as_ref().unwrap();
    let mut panels = vec![];
    root.panels(&mut panels);
    assert_eq!(panels, Panel::CONTENT);
    assert!(matches!(root, Dock::Split { ratio: 900, .. }));
    assert_eq!(layout.hidden, [Panel::Add]);
    assert_eq!(layout.hidden_controls, [Control::Action(Action::Undo)]);
    let before = layout.root.clone();
    let mut reopened: WorkspaceLayout =
        serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
    reopened.normalize();
    assert_eq!(reopened.root, before);
}

#[test]
fn resizing_in_layout_mode_persists_the_three_panel_arrangement() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.playlist_placement = Placement::Left;
    app.settings.player_placement = Placement::Bottom;
    ctrl_b(&mut app);
    render(&mut app, 120, 50);
    let split = app
        .view
        .workspace
        .splits
        .iter()
        .find(|split| split.axis == Axis::Horizontal)
        .unwrap()
        .clone();
    let x = split.area.x + split.cut - 1;
    let y = split.area.y + 5;
    app.handle(Input::Click {
        x,
        y,
        double: false,
    });
    assert!(app.view.workspace.gesture.is_some());
    app.handle(Input::Drag { x: x + 8, y });
    app.handle(Input::Release { x: x + 8, y });
    let expected = app.settings.workspace.root.clone();
    assert!(expected.is_some());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.store.settings().unwrap().workspace.root != expected {
        assert!(Instant::now() < deadline, "Layout was not saved");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(app);
    let reopened = App::new(&directory.0).unwrap();
    assert_eq!(reopened.settings.workspace.root, expected);
}

#[test]
fn border_geometry_applies_to_library_player_settings_and_command_menus() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    for (weight, corners, expected) in [
        (BorderWeight::Single, Corners::Square, Some("┌")),
        (BorderWeight::Single, Corners::Rounded, Some("╭")),
        (BorderWeight::Double, Corners::Square, Some("╔")),
        (BorderWeight::Thick, Corners::Square, Some("┏")),
        (BorderWeight::None, Corners::Square, None),
    ] {
        app.settings.appearance.borders = weight;
        app.settings.appearance.corners = corners;
        for (width, height) in [(120, 50), (32, 18)] {
            let terminal = render(&mut app, width, height);
            for (_, area) in &app.view.workspace.areas {
                let symbol = terminal.backend().buffer()[(area.x, area.y)].symbol();
                if let Some(expected) = expected {
                    assert_eq!(symbol, expected);
                } else {
                    assert!(!["┌", "╭", "╔", "┏"].contains(&symbol));
                }
            }
        }
        app.open_settings_page(SettingsPage::Typography);
        let terminal = render(&mut app, 120, 50);
        if let Some(expected) = expected {
            assert_eq!(terminal.backend().buffer()[(0, 0)].symbol(), expected);
        }
        key(&mut app, Key::Escape);
        app.view.dialog = Some(Dialog::Commands {
            menu: CommandMenu::Playlist,
            selected: 0,
            anchor: ratatui::layout::Position::new(2, 3),
        });
        let terminal = render(&mut app, 120, 50);
        let symbol = terminal.backend().buffer()[(2, 4)].symbol();
        if let Some(expected) = expected {
            assert_eq!(symbol, expected);
        } else {
            assert!(!["┌", "╭", "╔", "┏"].contains(&symbol));
        }
        key(&mut app, Key::Escape);
    }
}

#[test]
fn settings_show_compact_rows_and_one_selected_description() {
    let directory = Directory::new();
    let mut app = App::new(&directory.0).unwrap();
    app.settings.language = Language::Russian;
    app.open_settings_page(SettingsPage::General);
    let terminal = render(&mut app, 120, 50);
    let rows: Vec<_> = app
        .view
        .hits
        .iter()
        .filter(|hit| matches!(hit.target, Target::SettingSelect(_)))
        .collect();
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|hit| hit.area.height == 1));
    assert_eq!(rows[4].area.y - rows[0].area.y, 8);
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert_eq!(text.matches("Язык меню, описаний").count(), 1);
    assert!(!text.contains("Расположение списка плейлистов"));
    assert!(
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .all(|cell| !cell.modifier.contains(ratatui::style::Modifier::BOLD))
    );
    let scale = app
        .view
        .hits
        .iter()
        .find_map(|hit| match hit.target {
            Target::SettingsVolume(_, area) => Some(area),
            _ => None,
        })
        .unwrap();
    assert!(scale.width <= 24 && scale.height == 1);
    app.open_settings_page(SettingsPage::Typography);
    click(&mut app, |target| matches!(target, Target::Setting(1)));
    assert!(matches!(app.view.dialog, Some(Dialog::Text(_))));
    key(&mut app, Key::Escape);
    assert!(matches!(app.view.dialog, Some(Dialog::Settings { .. })));
}
