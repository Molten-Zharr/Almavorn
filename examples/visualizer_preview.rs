//! Deterministic previews of the real background compositor; no audio device needed.
use almavorn::{
    app::{App, CommandMenu, Target},
    audio::AnalysisFrame,
    input::{Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode},
    ui,
    visualizer::VisualizerKind,
};
use anyhow::Result;
use ratatui::Terminal;
use std::{
    f32::consts::TAU,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[path = "../src/gui_fonts.rs"]
mod gui_fonts;

fn render(app: &mut App, output: &Path, name: &str, columns: u16, rows: u16) -> Result<()> {
    let mut terminal = Terminal::new(gui_fonts::backend(
        app.settings.appearance.font,
        16,
        columns,
        rows,
    ))?;
    terminal.draw(|frame| ui::render(app, frame))?;
    let raster = terminal.backend();
    let mut file = fs::File::create(output.join(format!("{name}.ppm")))?;
    write!(
        file,
        "P6\n{} {}\n255\n",
        raster.get_pixmap_width(),
        raster.get_pixmap_height()
    )?;
    file.write_all(raster.get_pixmap_data())?;
    Ok(())
}
fn music(index: u64) -> AnalysisFrame {
    let t = index as f32 * 0.05;
    let pulse = (-((index % 10) as f32) * 0.45).exp();
    AnalysisFrame {
        sequence: index + 1,
        beats: index / 10 + 1,
        rms: 0.12 + pulse * 0.22,
        bass: 0.05 + pulse * 0.35,
        treble: 0.06 + (t * 3.0).sin().abs() * 0.08,
        bands: std::array::from_fn(|i| {
            (0.02 + pulse * 0.25) * (0.3 + (i as f32 * 0.7 + t).sin().abs() * 0.7)
        }),
        wave: std::array::from_fn(|i| (i as f32 * TAU / 16.0 + t).sin() * (0.12 + pulse * 0.6)),
    }
}
fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/visualizer-preview".into());
    fs::create_dir_all(&output)?;
    let directory = tempfile::tempdir()?;
    let mut app = App::new(directory.path())?;
    app.settings.language = Language::Russian;
    let change = app.store.create_playlist("Музыка", Mode::Chaos)?;
    let playlist = change
        .after
        .playlists
        .iter()
        .find(|p| p.name == "Музыка")
        .unwrap()
        .id;
    let tracks: Vec<_> = [
        "Hell March",
        "Soviet March",
        "Grinder",
        "На волнах",
        "Ночной город",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, title)| ImportedTrack {
        path: directory.path().join(format!("{index}.wav")),
        title: title.into(),
        artist: "Preview".into(),
        album: "Music dynamics".into(),
        duration_ms: 180_000,
        tags: "{}".into(),
    })
    .collect();
    app.store.add_tracks(playlist, &tracks, false)?;
    app.set_mode(Mode::Chaos)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.busy() {
        anyhow::ensure!(Instant::now() < deadline, "Library preview timed out");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    app.library.selected_playlist = Some(playlist);
    for index in 0..80 {
        app.playback
            .visualizer
            .advance(Some(music(index)), 0.05, true);
    }
    for kind in VisualizerKind::ALL {
        app.settings.appearance.visualizer = kind;
        render(&mut app, &output, &format!("{kind:?}"), 140, 45)?;
    }
    app.settings.appearance.visualizer = VisualizerKind::Bombardment;
    render(&mut app, &output, "bombardment-compact", 80, 30)?;
    render(&mut app, &output, "bombardment-small", 32, 26)?;
    for (columns, rows, name) in [
        (140, 45, "animation-menu"),
        (32, 26, "animation-menu-small"),
    ] {
        render(&mut app, &output, &format!("{name}-closed"), columns, rows)?;
        let button = app
            .view
            .hits
            .iter()
            .find(|hit| matches!(hit.target, Target::CommandMenu(CommandMenu::Visualizer)))
            .expect("VIS dropdown button")
            .area;
        app.handle(Input::Click {
            x: button.x,
            y: button.y,
            double: false,
        });
        render(&mut app, &output, name, columns, rows)?;
        app.handle(Input::Key(KeyPress::plain(Key::Escape)));
    }
    for index in 80..140 {
        app.playback
            .visualizer
            .advance(Some(music(index)), 0.05, true);
        render(
            &mut app,
            &output,
            &format!("bombardment-{:03}", index - 80),
            100,
            35,
        )?;
    }
    app.settings.appearance.palette_ids = ["light".into(), "light".into()];
    render(&mut app, &output, "bombardment-light", 140, 45)?;
    println!("Background previews: {}", output.display());
    Ok(())
}
