//! Render the shared listening-statistics dialog without an audio device/window.
use almavorn::{
    app::{App, StatisticsPage, Target},
    input::{Action, Input},
    model::{ImportedTrack, Language},
    statistics::ListeningRecord,
    ui,
};
use anyhow::{Result, ensure};
use ratatui::Terminal;
use std::{
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
fn load(app: &mut App) -> Result<()> {
    app.action(Action::Statistics)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.statistics.view.loading {
        ensure!(Instant::now() < deadline, "Statistics preview timed out");
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/statistics-preview".into());
    fs::create_dir_all(&output)?;
    let directory = tempfile::tempdir()?;
    let mut app = App::new(directory.path())?;
    app.settings.language = Language::Russian;
    load(&mut app)?;
    render(&mut app, &output, "empty", 110, 36)?;
    let playlist = app.store.playlists()?[0].id;
    let tracks = [
        ("Hell March", "Frank Klepacki", "Red Alert"),
        ("Soviet March", "James Hannigan", "Red Alert 3"),
        ("Grinder", "Frank Klepacki", "Red Alert 2"),
        ("Ночной город", "Кино", "Группа крови"),
        ("Спокойная ночь", "Кино", "Группа крови"),
        ("На волнах", "Молчат Дома", "Этажи"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, artist, album))| ImportedTrack {
        path: directory.path().join(format!("{index}.wav")),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration_ms: 240_000,
        tags: "{}".into(),
    })
    .collect::<Vec<_>>();
    app.store.add_tracks(playlist, &tracks, true)?;
    let mut records = Vec::new();
    for (index, entry) in app.store.playlists()?[0].entries.iter().enumerate() {
        for repeat in 0..(20 - index * 2) {
            let heard = if repeat % 6 == 0 { 30_000 } else { 240_000 };
            let started = 1_791_650_000_000 + (index * 20 + repeat) as i64 * 300_000;
            records.push(ListeningRecord {
                session_id: format!("preview-{index}-{repeat}"),
                track_id: entry.track.id,
                duration_ms: 240_000,
                started_at_ms: started,
                updated_at_ms: started + heard as i64,
                listened_ms: heard,
                qualified: heard >= 120_000,
                ended_at_ms: Some(started + heard as i64),
            });
        }
    }
    app.store.record_listening(&records)?;
    load(&mut app)?;
    for (page, name) in [
        (StatisticsPage::Tracks, "tracks"),
        (StatisticsPage::Artists, "artists"),
        (StatisticsPage::Albums, "albums"),
        (StatisticsPage::Recent, "recent"),
    ] {
        render(&mut app, &output, "controls", 110, 36)?;
        let hit = app
            .view
            .hits
            .iter()
            .find(|hit| matches!(hit.target, Target::StatisticsPage(value) if value == page))
            .unwrap()
            .area;
        app.handle(Input::Click {
            x: hit.x,
            y: hit.y,
            double: false,
        });
        render(&mut app, &output, name, 110, 36)?;
        render(&mut app, &output, &format!("{name}-small"), 32, 26)?;
    }
    app.settings.appearance.palette_ids = ["light".into(), "light".into()];
    render(&mut app, &output, "recent-light", 110, 36)?;
    println!("Listening statistics previews: {}", output.display());
    Ok(())
}
