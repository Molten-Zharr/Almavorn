use almavorn::{
    app::{App, Target},
    input::Action,
    model::{Language, Placement},
    preferences::{BorderWeight, FontFace},
    ui,
    workspace::{Axis, Control, Dock, Panel},
};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn workspace_rendering_and_interaction_geometry_match_before_extraction() {
    // Baseline recorded on d281a2e, before extracting the layout engine.
    let expected = [
        16273899027879206205,
        11586439943453613586,
        11233230641664525085,
        12695554178879198995,
        2398738410566443368,
        7146151675084106417,
        300776342566576031,
        6717591284664557950,
    ];
    let mut actual = Vec::new();
    for scenario in 0..8 {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        match scenario {
            1 => {
                app.settings.preferences.language = Language::English;
                app.settings.preferences.appearance.font = FontFace::FiraCodeBold;
                app.view.layout_editing = true;
            }
            2 => {
                app.settings.preferences.appearance.font = FontFace::DejaVuMono;
                app.settings.preferences.player_placement = Placement::Left;
                app.settings.preferences.playlist_placement = Placement::Bottom;
            }
            3 => {
                app.settings.preferences.workspace.hidden = vec![Panel::Playlists];
                app.settings.preferences.workspace.collapsed = vec![Panel::Player];
            }
            4 => {
                app.settings.preferences.workspace.root = Some(Dock::split(
                    Axis::Horizontal,
                    500,
                    Dock::Panel(Panel::Player),
                    Dock::split(
                        Axis::Vertical,
                        500,
                        Dock::Panel(Panel::Playlists),
                        Dock::Panel(Panel::Tracks),
                    ),
                ));
                app.settings.preferences.workspace.collapsed = vec![Panel::Tracks];
                app.settings.preferences.workspace.hidden_controls =
                    vec![Control::Action(Action::NewPlaylist)];
                app.view.layout_editing = true;
            }
            5 => {
                app.settings.preferences.workspace.hidden = vec![Panel::Playlists];
                app.settings.preferences.workspace.collapsed = Panel::CONTENT.to_vec();
            }
            6 => app.settings.preferences.appearance.borders = BorderWeight::None,
            7 => app.settings.preferences.workspace.hidden = Panel::CONTENT.to_vec(),
            _ => app.settings.preferences.appearance.show_button_shortcuts = false,
        }
        let mut hash = 0xcbf29ce484222325u64;
        for (width, height) in [
            (32, 18),
            (32, 26),
            (54, 30),
            (55, 30),
            (79, 30),
            (80, 30),
            (120, 50),
            (140, 45),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::render(&mut app, frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let mut snapshot = format!("{width}x{height}:{:?}", app.view.workspace.areas);
            for cell in &buffer.content {
                snapshot.push_str(&format!(
                    "{}:{:?}:{:?}:{:?};",
                    cell.symbol(),
                    cell.fg,
                    cell.bg,
                    cell.modifier
                ));
            }
            for split in &app.view.workspace.splits {
                snapshot.push_str(&format!(
                    "{:?}:{:?}:{:?}:{}:{:?};",
                    split.path, split.area, split.axis, split.cut, split.minimum
                ));
            }
            for hit in &app.view.hits {
                if matches!(
                    hit.target,
                    Target::PanelResize(_)
                        | Target::PanelFocus(_)
                        | Target::Seek(_)
                        | Target::PlaybackVolume(_)
                ) {
                    snapshot.push_str(&format!("{:?}:{};", hit.area, hit.enabled));
                }
            }
            for byte in snapshot.bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        actual.push(hash);
    }
    assert_eq!(actual.as_slice(), expected);
}
