//! Render settings with the desktop font backend into portable PPM images.
use almavorn::{
    app::{App, SettingsPage},
    ui,
};
use anyhow::Result;
use ratatui::Terminal;
use std::{fs, io::Write, path::PathBuf};

#[path = "../src/gui_fonts.rs"]
mod gui_fonts;

fn main() -> Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/settings-preview"));
    fs::create_dir_all(&output)?;
    let data = output.join("data");
    let mut app = App::new(&data)?;
    for (index, page) in SettingsPage::ALL.into_iter().enumerate() {
        app.open_settings_page(page);
        let backend = gui_fonts::backend(
            app.settings.appearance.font,
            app.settings.appearance.font_size,
            120,
            50,
        );
        let mut terminal = Terminal::new(backend)?;
        terminal.draw(|frame| ui::render(&mut app, frame))?;
        let raster = terminal.backend();
        let mut file = fs::File::create(output.join(format!("{}-{:?}.ppm", index + 1, page)))?;
        write!(
            file,
            "P6\n{} {}\n255\n",
            raster.get_pixmap_width(),
            raster.get_pixmap_height()
        )?;
        file.write_all(raster.get_pixmap_data())?;
    }
    println!("Settings previews: {}", output.display());
    Ok(())
}
