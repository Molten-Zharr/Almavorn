//! Render settings with the desktop font backend into portable PPM images.
use almavorn::{
    app::{App, SettingsPage},
    ui,
};
use anyhow::Result;
use ratatui::Terminal;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
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
        .unwrap_or_else(|| PathBuf::from("target/settings-preview"));
    fs::create_dir_all(&output)?;
    let data = output.join(format!("data-{}", std::process::id()));
    let mut app = App::new(&data)?;
    if let Some(palette) = std::env::args().nth(2) {
        anyhow::ensure!(
            app.settings.palettes.iter().any(|item| item.id == palette),
            "Unknown preview palette: {palette}"
        );
        app.settings.appearance.palette_ids = [palette.clone(), palette];
    }
    for (index, page) in SettingsPage::ALL.into_iter().enumerate() {
        app.open_settings_page(page);
        render(
            &mut app,
            &output,
            &format!("{}-{page:?}", index + 1),
            140,
            45,
        )?;
    }
    app.open_settings_page(SettingsPage::General);
    render(&mut app, &output, "compact-general", 70, 30)?;
    app.open_settings_page(SettingsPage::Typography);
    render(&mut app, &output, "compact-fonts", 44, 22)?;
    drop(app);
    fs::remove_dir_all(data)?;
    println!("Settings previews: {}", output.display());
    Ok(())
}
