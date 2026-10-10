use almavorn::{
    app::{App, CommandMenu, Dialog, SettingsPage, Target},
    audio::{AnalysisFrame, SPECTRUM_BANDS, WAVE_SAMPLES},
    input::{Action, Input, Key, KeyPress},
    model::Language,
    preferences::Appearance,
    ui,
    visualizer::VisualizerKind,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(app, frame)).unwrap();
    terminal.backend().buffer().clone()
}
fn music() -> AnalysisFrame {
    AnalysisFrame {
        sequence: 1,
        beats: 1,
        rms: 0.4,
        bass: 0.3,
        treble: 0.2,
        bands: [0.25; SPECTRUM_BANDS],
        wave: std::array::from_fn(|i| (i as f32 / WAVE_SAMPLES as f32 * 12.0).sin() * 0.7),
    }
}
fn animated_dots(buffer: &Buffer) -> usize {
    buffer
        .content
        .iter()
        .filter(|cell| {
            cell.symbol()
                .chars()
                .any(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
        })
        .count()
}

#[test]
fn every_scene_is_a_background_and_preserves_text_selection_and_hit_areas() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    app.playback.visualizer.advance(Some(music()), 0.05, true);
    for palette in ["molten-zharr", "light"] {
        app.settings.appearance.palette_ids = [palette.into(), palette.into()];
        for (width, height) in [(32, 26), (80, 30), (140, 45)] {
            for kind in VisualizerKind::ALL
                .into_iter()
                .filter(|kind| *kind != VisualizerKind::Off)
            {
                app.settings.appearance.visualizer = kind;
                app.playback.visualizer.available = false;
                let plain = render(&mut app, width, height);
                let hits: Vec<_> = app
                    .view
                    .hits
                    .iter()
                    .map(|hit| (hit.area, hit.enabled))
                    .collect();
                app.playback.visualizer.available = true;
                let animated = render(&mut app, width, height);
                assert!(
                    animated_dots(&animated) > 0,
                    "{palette} {width}×{height} {kind:?}"
                );
                for (before, after) in plain.content.iter().zip(&animated.content) {
                    if before.symbol() != " " {
                        assert_eq!(
                            before.symbol(),
                            after.symbol(),
                            "{kind:?}: foreground text changed"
                        );
                        assert_eq!(before.fg, after.fg);
                        assert_eq!(before.modifier, after.modifier);
                    }
                    // Selection and button backgrounds retain their contrast.
                    let [r, g, b] = app.settings.current_palette().color("background");
                    if before.bg != ratatui::style::Color::Rgb(r, g, b) {
                        assert_eq!(before, after);
                    }
                }
                assert_eq!(
                    hits,
                    app.view
                        .hits
                        .iter()
                        .map(|hit| (hit.area, hit.enabled))
                        .collect::<Vec<_>>()
                );
                assert!(
                    app.view
                        .hits
                        .iter()
                        .all(
                            |hit| hit.area.intersection(Rect::new(0, 0, width, height)) == hit.area
                        )
                );
            }
        }
    }
}

#[test]
fn settings_and_dialogs_cover_the_animation_and_pause_keeps_the_same_frame() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = App::new(directory.path()).unwrap();
    app.playback.visualizer.advance(Some(music()), 0.05, true);
    let playing = render(&mut app, 120, 40);
    app.playback.visualizer.advance(
        Some(AnalysisFrame {
            sequence: 2,
            beats: 2,
            ..music()
        }),
        0.1,
        false,
    );
    assert_eq!(playing, render(&mut app, 120, 40));
    app.open_settings_page(SettingsPage::General);
    assert_eq!(animated_dots(&render(&mut app, 120, 40)), 0);
    app.view.dialog = Some(Dialog::Help { offset: 0 });
    let animated = render(&mut app, 120, 40);
    let overlay = app.view.overlay_area;
    app.settings.appearance.visualizer = VisualizerKind::Off;
    let plain = render(&mut app, 120, 40);
    assert!(overlay.width > 0 && overlay.height > 0);
    for y in overlay.y..overlay.bottom() {
        for x in overlay.x..overlay.right() {
            assert_eq!(animated[(x, y)], plain[(x, y)]);
        }
    }
}

#[test]
fn shortcut_and_settings_save_the_animation_in_the_active_profile() {
    let directory = tempfile::tempdir().unwrap();
    {
        let mut app = App::new(directory.path()).unwrap();
        assert_eq!(
            app.settings.appearance.visualizer,
            VisualizerKind::Bombardment
        );
        app.handle(Input::Key(KeyPress::plain(Key::F(6))));
        assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Off);
        app.action(Action::CycleVisualizer).unwrap();
        assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Spectrum);
        app.open_settings_page(SettingsPage::General);
        for target in [(7, 1), (8, 1)] {
            render(&mut app, 120, 50);
            let area = app.view.hits.iter().find(|hit| matches!(hit.target, Target::SettingAdjust(row, direction) if (row, direction) == target)).unwrap().area;
            app.handle(Input::Click {
                x: area.x,
                y: area.y,
                double: false,
            });
        }
        assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Wave);
        assert_eq!(app.settings.appearance.animation_brightness, 40);
        let profile = app
            .settings
            .profiles
            .iter()
            .find(|p| p.id == app.settings.active_profile)
            .unwrap();
        assert_eq!(
            profile.preferences.appearance.visualizer,
            VisualizerKind::Wave
        );
        assert_eq!(profile.preferences.appearance.animation_brightness, 40);
    }
    let app = App::new(directory.path()).unwrap();
    assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Wave);
    assert_eq!(app.settings.appearance.animation_brightness, 40);
    let legacy: Appearance = serde_json::from_str("{}").unwrap();
    assert_eq!(legacy.visualizer, VisualizerKind::Bombardment);
    assert_eq!(legacy.animation_brightness, 35);
}

#[test]
fn vis_dropdown_marks_the_current_scene_and_supports_mouse_keyboard_and_cancel() {
    let directory = tempfile::tempdir().unwrap();
    {
        let mut app = App::new(directory.path()).unwrap();
        for language in [Language::Russian, Language::English] {
            app.settings.language = language;
            for (width, height) in [(32, 26), (80, 30), (140, 45)] {
                app.settings.appearance.visualizer = VisualizerKind::Bombardment;
                render(&mut app, width, height);
                let button = app
                    .view
                    .hits
                    .iter()
                    .find(|hit| matches!(hit.target, Target::CommandMenu(CommandMenu::Visualizer)))
                    .unwrap()
                    .area;
                app.handle(Input::Click {
                    x: button.x,
                    y: button.y,
                    double: false,
                });
                assert_eq!(
                    app.settings.appearance.visualizer,
                    VisualizerKind::Bombardment,
                    "Opening VIS must keep the playing scene"
                );
                assert!(matches!(
                    app.view.dialog,
                    Some(Dialog::Commands {
                        menu: CommandMenu::Visualizer,
                        selected: 4,
                        ..
                    })
                ));
                render(&mut app, width, height);
                let items = app.command_items(CommandMenu::Visualizer);
                let kinds: Vec<_> = items
                    .iter()
                    .map(|item| match item.target {
                        Target::Visualizer(kind) => kind,
                        _ => panic!("Expected a scene choice"),
                    })
                    .collect();
                assert_eq!(
                    kinds,
                    [
                        VisualizerKind::Spectrum,
                        VisualizerKind::Wave,
                        VisualizerKind::Rings,
                        VisualizerKind::Particles,
                        VisualizerKind::Bombardment,
                        VisualizerKind::Off
                    ]
                );
                assert!(items[4].label.starts_with('●'));
                assert_eq!(
                    items
                        .iter()
                        .filter(|item| item.label.starts_with('●'))
                        .count(),
                    1
                );
                assert!(
                    app.view
                        .hits
                        .iter()
                        .all(
                            |hit| hit.area.intersection(Rect::new(0, 0, width, height)) == hit.area
                        )
                );
                let wave = app
                    .view
                    .hits
                    .iter()
                    .find(|hit| matches!(hit.target, Target::CommandRow(1)))
                    .unwrap()
                    .area;
                app.handle(Input::Click {
                    x: wave.x,
                    y: wave.y,
                    double: false,
                });
                assert!(app.view.dialog.is_none());
                assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Wave);
                render(&mut app, width, height);
                app.handle(Input::Click {
                    x: button.x,
                    y: button.y,
                    double: false,
                });
                assert!(matches!(
                    app.view.dialog,
                    Some(Dialog::Commands { selected: 1, .. })
                ));
                for _ in 0..4 {
                    app.handle(Input::Key(KeyPress::plain(Key::Down)));
                }
                app.handle(Input::Key(KeyPress::plain(Key::Enter)));
                assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Off);
                render(&mut app, width, height);
                app.handle(Input::Click {
                    x: button.x,
                    y: button.y,
                    double: false,
                });
                for _ in 0..5 {
                    app.handle(Input::Key(KeyPress::plain(Key::Up)));
                }
                app.handle(Input::Key(KeyPress::plain(Key::Escape)));
                assert!(app.view.dialog.is_none());
                assert_eq!(
                    app.settings.appearance.visualizer,
                    VisualizerKind::Off,
                    "Esc must keep the saved scene"
                );
                app.handle(Input::Key(KeyPress::plain(Key::F(6))));
                assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Spectrum);
            }
        }
        let profile = app
            .settings
            .profiles
            .iter()
            .find(|profile| profile.id == app.settings.active_profile)
            .unwrap();
        assert_eq!(
            profile.preferences.appearance.visualizer,
            VisualizerKind::Spectrum
        );
    }
    let app = App::new(directory.path()).unwrap();
    assert_eq!(app.settings.appearance.visualizer, VisualizerKind::Spectrum);
}
