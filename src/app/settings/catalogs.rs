use super::{App, SettingsCatalog, SettingsEdit, SettingsPage};
use crate::{
    app::Dialog,
    preferences::{
        MAX_FONT_SIZE, MIN_FONT_SIZE, SettingsProfile, ThemePreset, parse_color, validate_name,
    },
};
use anyhow::{Context, Result, ensure};

fn available_name<'a>(
    name: &str,
    current: Option<&str>,
    entries: impl Iterator<Item = (&'a str, &'a str)>,
) -> Result<String> {
    let name = name.trim();
    validate_name(name)?;
    ensure!(
        !entries
            .into_iter()
            .any(|(id, existing)| Some(id) != current
                && existing.to_lowercase() == name.to_lowercase()),
        "This name is already used"
    );
    Ok(name.into())
}
impl App {
    pub(crate) fn finish_settings_edit(&mut self, edit: SettingsEdit, text: &str) -> Result<()> {
        match edit {
            SettingsEdit::CreateProfile => {
                let name = available_name(
                    text,
                    None,
                    self.settings
                        .profiles
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                let id = self.settings.fresh_id("profile");
                self.settings.profiles.push(SettingsProfile {
                    id: id.clone(),
                    name,
                    preferences: self.settings.profile_preferences(),
                });
                self.settings.active_profile = id;
                self.settings_view.profile = self.settings.profiles.len() - 1;
            }
            SettingsEdit::RenameProfile(id) => {
                let name = available_name(
                    text,
                    Some(&id),
                    self.settings
                        .profiles
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                self.settings
                    .profiles
                    .iter_mut()
                    .find(|p| p.id == id)
                    .context("Profile no longer exists")?
                    .name = name;
            }
            SettingsEdit::CreatePalette => {
                let name = available_name(
                    text,
                    None,
                    self.settings
                        .palettes
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                let mut palette = self.selected_settings_palette().clone();
                palette.id = self.settings.fresh_id("palette");
                palette.name = name;
                self.settings.palettes.push(palette);
                self.settings_view.palette = self.settings.palettes.len() - 1;
            }
            SettingsEdit::RenamePalette(id) => {
                let name = available_name(
                    text,
                    Some(&id),
                    self.settings
                        .palettes
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                self.settings
                    .palettes
                    .iter_mut()
                    .find(|p| p.id == id)
                    .context("Palette no longer exists")?
                    .name = name;
            }
            SettingsEdit::PaletteColor { id, token } => {
                let color = parse_color(text)?;
                let palette = self
                    .settings
                    .palettes
                    .iter_mut()
                    .find(|p| p.id == id)
                    .context("Palette no longer exists")?;
                ensure!(palette.colors.contains_key(&token), "Unknown palette color");
                palette.colors.insert(token, color);
            }
            SettingsEdit::CreatePreset => {
                let name = available_name(
                    text,
                    None,
                    self.settings
                        .presets
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                let id = self.settings.fresh_id("preset");
                let style = self.settings.current_style();
                self.settings.presets.push(ThemePreset { id, name, style });
                self.settings_view.preset = self.settings.presets.len() - 1;
            }
            SettingsEdit::RenamePreset(id) => {
                let name = available_name(
                    text,
                    Some(&id),
                    self.settings
                        .presets
                        .iter()
                        .map(|p| (p.id.as_str(), p.name.as_str())),
                )?;
                self.settings
                    .presets
                    .iter_mut()
                    .find(|p| p.id == id)
                    .context("Preset no longer exists")?
                    .name = name;
            }
            SettingsEdit::FontSize => {
                let size: u16 = text
                    .trim()
                    .parse()
                    .context("Font size must be 10-32 pixels")?;
                ensure!(
                    (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size),
                    "Font size must be 10-32 pixels"
                );
                self.settings.appearance.font_size = size;
            }
            SettingsEdit::ImportPalette => {
                self.start_palette_import(text)?;
                self.show_settings();
                return Ok(());
            }
            SettingsEdit::ExportPalette(id) => {
                self.start_palette_export(&id, text)?;
                self.show_settings();
                return Ok(());
            }
        }
        self.save_settings()?;
        self.show_settings();
        self.message(self.text("Settings saved", "Настройки сохранены").into());
        Ok(())
    }
    pub(super) fn apply_selected_profile(&mut self) -> Result<()> {
        let profile = self.settings.profiles[self
            .settings_view
            .profile
            .min(self.settings.profiles.len() - 1)]
        .clone();
        self.settings.sync_active_profile();
        self.settings.apply_preferences(profile.preferences);
        self.settings.active_profile = profile.id;
        if let Some(audio) = &self.audio {
            audio.volume(self.settings.volume);
        }
        self.save_settings()?;
        self.show_settings();
        self.message(self.text("Profile activated", "Профиль включён").into());
        Ok(())
    }
    pub(super) fn settings_catalog_shortcut(&mut self, rename: bool) -> Result<()> {
        use super::SettingControl as C;
        let control = match (self.settings_view.page, rename) {
            (SettingsPage::Profiles, false) => C::ProfileCreate,
            (SettingsPage::Profiles, true) => C::ProfileRename,
            (SettingsPage::Palettes, false) => C::PaletteCreate,
            (SettingsPage::Palettes, true) => C::PaletteRename,
            (SettingsPage::Themes, false) => C::PresetCreate,
            (SettingsPage::Themes, true) => C::PresetRename,
            _ => return Ok(()),
        };
        if let Some(index) = self.settings_rows().iter().position(|row| {
            std::mem::discriminant(&row.control) == std::mem::discriminant(&control)
        }) {
            self.setting(index)?;
        }
        Ok(())
    }
    pub(super) fn request_settings_delete(&mut self) -> Result<()> {
        let (catalog, id, name, length) = match self.settings_view.page {
            SettingsPage::Profiles => {
                let p = &self.settings.profiles[self
                    .settings_view
                    .profile
                    .min(self.settings.profiles.len() - 1)];
                (
                    SettingsCatalog::Profile,
                    p.id.clone(),
                    p.name.clone(),
                    self.settings.profiles.len(),
                )
            }
            SettingsPage::Palettes => {
                let p = self.selected_settings_palette();
                (
                    SettingsCatalog::Palette,
                    p.id.clone(),
                    p.name.clone(),
                    self.settings.palettes.len(),
                )
            }
            SettingsPage::Themes => {
                let p = &self.settings.presets[self
                    .settings_view
                    .preset
                    .min(self.settings.presets.len() - 1)];
                (
                    SettingsCatalog::Preset,
                    p.id.clone(),
                    p.name.clone(),
                    self.settings.presets.len(),
                )
            }
            _ => return Ok(()),
        };
        ensure!(length > 1, "Keep at least one item in this catalog");
        self.dialog = Some(Dialog::ConfirmSettings { catalog, id, name });
        Ok(())
    }
    pub(crate) fn delete_settings_item(
        &mut self,
        catalog: SettingsCatalog,
        id: &str,
    ) -> Result<()> {
        match catalog {
            SettingsCatalog::Palette => {
                ensure!(
                    self.settings.palettes.iter().any(|p| p.id == id),
                    "Palette no longer exists"
                );
                self.settings.remove_palette(id)?;
                self.settings_view.palette = self
                    .settings_view
                    .palette
                    .min(self.settings.palettes.len() - 1);
            }
            SettingsCatalog::Preset => {
                ensure!(
                    self.settings.presets.len() > 1,
                    "Keep at least one theme preset"
                );
                ensure!(
                    self.settings.presets.iter().any(|p| p.id == id),
                    "Preset no longer exists"
                );
                self.settings.presets.retain(|p| p.id != id);
                self.settings_view.preset = self
                    .settings_view
                    .preset
                    .min(self.settings.presets.len() - 1);
            }
            SettingsCatalog::Profile => {
                ensure!(
                    self.settings.profiles.len() > 1,
                    "Keep at least one profile"
                );
                ensure!(
                    self.settings.profiles.iter().any(|p| p.id == id),
                    "Profile no longer exists"
                );
                self.settings.profiles.retain(|p| p.id != id);
                self.settings_view.profile = self
                    .settings_view
                    .profile
                    .min(self.settings.profiles.len() - 1);
                if self.settings.active_profile == id {
                    self.settings_view.profile = 0;
                    self.apply_selected_profile()?;
                }
            }
        }
        self.save_settings()?;
        self.show_settings();
        self.message(
            self.text("Removed from settings", "Удалено из настроек")
                .into(),
        );
        Ok(())
    }
}
