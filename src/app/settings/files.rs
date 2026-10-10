use super::App;
use crate::errors::AppError;
use crate::{
    app::background::{Background, background, poll},
    preferences::NamedPalette,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

enum FileOutcome {
    Imported(NamedPalette),
    Exported(PathBuf),
}
pub(crate) struct SettingsFileJob {
    receiver: Background<FileOutcome>,
}
fn export_filename(text: &str) -> Result<String> {
    let name = text.trim();
    ensure!(
        !name.is_empty()
            && !matches!(name, "." | "..")
            && !name
                .chars()
                .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c)),
        AppError::InvalidFileName
    );
    let name = if Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        name.to_owned()
    } else {
        format!("{name}.json")
    };
    ensure!(name.len() <= 255, AppError::InvalidFileName);
    Ok(name)
}
impl App {
    pub fn settings_file_busy(&self) -> bool {
        self.persistence.file.is_some()
    }
    pub(crate) fn start_palette_import(&mut self, path: &Path) -> Result<()> {
        ensure!(!self.settings_file_busy(), AppError::PaletteFileBusy);
        let path = path.to_owned();
        self.persistence.file = Some(SettingsFileJob {
            receiver: background(self.runtime(), move || {
                let file =
                    File::open(&path).with_context(|| format!("Cannot open {}", path.display()))?;
                let mut bytes = Vec::new();
                file.take(1_048_577).read_to_end(&mut bytes)?;
                Ok(FileOutcome::Imported(NamedPalette::import_json(&bytes)?))
            }),
        });
        self.message(
            self.text("Importing palette...", "Импорт палитры...")
                .into(),
        );
        Ok(())
    }
    pub(super) fn start_palette_export(
        &mut self,
        id: &str,
        directory: &Path,
        text: &str,
    ) -> Result<()> {
        ensure!(!self.settings_file_busy(), AppError::PaletteFileBusy);
        let path = directory.join(export_filename(text)?);
        let bytes = self
            .settings
            .palettes
            .iter()
            .find(|p| p.id == id)
            .context(AppError::PaletteMissing)?
            .export_json()?;
        self.persistence.file = Some(SettingsFileJob {
            receiver: background(self.runtime(), move || {
                write_new_file(&path, &bytes)?;
                Ok(FileOutcome::Exported(path))
            }),
        });
        self.message(
            self.text("Exporting palette...", "Экспорт палитры...")
                .into(),
        );
        Ok(())
    }
    pub(crate) fn tick_settings_files(&mut self) -> Result<()> {
        let Some(result) = self
            .persistence
            .file
            .as_mut()
            .and_then(|job| poll(&mut job.receiver))
        else {
            return Ok(());
        };
        self.persistence.file = None;
        match result? {
            FileOutcome::Imported(mut palette) => {
                palette.id = self.settings.fresh_id("palette");
                let original_name = palette.name.clone();
                let mut suffix = 2;
                while self
                    .settings
                    .palettes
                    .iter()
                    .any(|p| p.name.to_lowercase() == palette.name.to_lowercase())
                {
                    let stem: String = original_name.chars().take(68).collect();
                    palette.name = format!("{stem} ({suffix})");
                    suffix += 1;
                }
                self.settings.palettes.push(palette);
                self.view.settings.palette = self.settings.palettes.len() - 1;
                self.save_settings()?;
                self.message(
                    self.text("Palette imported", "Палитра импортирована")
                        .into(),
                );
            }
            FileOutcome::Exported(path) => self.message(format!(
                "{}: {}",
                self.text("Palette exported", "Палитра экспортирована"),
                path.display()
            )),
        }
        Ok(())
    }
}
fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Cannot create {}. Choose a new file name", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}
