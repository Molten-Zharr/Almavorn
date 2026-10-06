//! Render the real desktop interface into PPM images without a display or audio device.
use almavorn::{
    app::{App, CommandMenu, Dialog},
    input::{Action, Input, Key, KeyPress},
    model::{ImportedTrack, Language, Mode, Placement},
    ui,
};
use anyhow::Result;
use ratatui::{Terminal, layout::Position};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[path = "../src/gui_fonts.rs"]
mod gui_fonts;

fn render(app: &mut App, output: &Path, name: &str, columns: u16, rows: u16) -> Result<()> {
    let backend = gui_fonts::backend(
        app.settings.appearance.font,
        app.settings.appearance.font_size,
        columns,
        rows,
    );
    let mut terminal = Terminal::new(backend)?;
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

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/interface-preview".into());
    fs::create_dir_all(&output)?;
    let data = output.join(format!("data-{}", std::process::id()));
    let mut app = App::new(&data)?;
    app.settings.language = Language::Russian;
    app.settings.appearance.palette_ids = ["classic-violet".into(), "classic-violet".into()];
    app.settings.playlist_placement = Placement::Left;
    app.settings.player_placement = Placement::Bottom;
    render(&mut app, &output, "empty", 140, 45)?;
    app.set_mode(Mode::Chaos)?;
    app.action(Action::ToggleDesk)?;
    let change = app.store.create_playlist("Вечерняя музыка", Mode::Chaos)?;
    let playlist = change
        .after
        .playlists
        .iter()
        .find(|playlist| playlist.name == "Вечерняя музыка")
        .expect("Created playlist exists")
        .id;
    let tracks: Vec<_> = [
        ("Звёздный свет", "Север", "Тихий город"),
        ("Дорога домой", "Полюс", "Горизонт"),
        ("После дождя", "Север", "Тихий город"),
        ("Ночной поезд", "Маяк", "Путешествие"),
        ("Тёплый ветер", "Полюс", "Горизонт"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, artist, album))| ImportedTrack {
        path: data.join(format!("preview-{index}.wav")),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration_ms: 180_000 + index as u64 * 14_000,
        tags: "{}".into(),
    })
    .collect();
    app.store.add_tracks(playlist, &tracks, false)?;
    app.set_mode(Mode::Chaos)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.busy() {
        anyhow::ensure!(
            Instant::now() < deadline,
            "Preview library did not finish loading"
        );
        app.tick();
        std::thread::sleep(Duration::from_millis(1));
    }
    render(&mut app, &output, "library", 140, 45)?;
    render(&mut app, &output, "compact", 80, 30)?;
    render(&mut app, &output, "small", 32, 18)?;
    render(&mut app, &output, "library", 140, 45)?;
    for _ in 0..=app.view.workspace.areas.len() {
        if app.view.toolbar_selected.is_some() {
            break;
        }
        app.handle(Input::Key(KeyPress::plain(Key::Tab)));
    }
    app.handle(Input::Key(KeyPress::plain(Key::Home)));
    render(&mut app, &output, "keyboard-focus", 140, 45)?;
    app.handle(Input::Key(KeyPress::plain(Key::Enter)));
    render(&mut app, &output, "keyboard-menu", 140, 45)?;
    app.handle(Input::Key(KeyPress::plain(Key::Escape)));
    app.handle(Input::Key(KeyPress::plain(Key::Escape)));
    app.view.dialog = Some(Dialog::Commands {
        menu: CommandMenu::Tracks,
        selected: 2,
        anchor: Position::new(70, 5),
    });
    render(&mut app, &output, "menu", 140, 45)?;
    app.view.dialog = None;
    app.view.layout_editing = true;
    render(&mut app, &output, "layout", 140, 45)?;
    app.view.layout_editing = false;
    app.action(Action::Help)?;
    render(&mut app, &output, "help-start", 140, 45)?;
    for _ in 0..3 {
        app.handle(Input::Key(KeyPress::plain(Key::PageDown)));
    }
    render(&mut app, &output, "help-middle", 140, 45)?;
    for _ in 0..100 {
        app.handle(Input::Key(KeyPress::plain(Key::PageDown)));
    }
    render(&mut app, &output, "help-bottom", 140, 45)?;
    drop(app);
    fs::remove_dir_all(data)?;
    println!("Interface previews: {}", output.display());
    Ok(())
}
