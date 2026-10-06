//! Persistent settings catalogs and portable palette files, independent of UI state.
use crate::errors::AppError;
use crate::{
    input::Binding,
    model::{Language, Mode, Placement, Settings, Theme},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    sync::OnceLock,
};

pub const BRAND_PALETTE: &str = "molten-zharr";
pub const MIN_FONT_SIZE: u16 = 10;
pub const MAX_FONT_SIZE: u16 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFace {
    FiraCodeBold,
    #[default]
    FiraCode,
    DejaVuMono,
}
impl FontFace {
    pub const ALL: [Self; 3] = [Self::FiraCode, Self::FiraCodeBold, Self::DejaVuMono];
    pub fn name(self) -> &'static str {
        match self {
            Self::FiraCodeBold => "Fira Code Bold",
            Self::FiraCode => "Fira Code",
            Self::DejaVuMono => "DejaVu Sans Mono",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderWeight {
    None,
    #[default]
    Single,
    Double,
    Thick,
}
impl BorderWeight {
    pub const ALL: [Self; 4] = [Self::None, Self::Single, Self::Double, Self::Thick];
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::None => language.text("None", "Без рамки"),
            Self::Single => language.text("Single", "Одинарная"),
            Self::Double => language.text("Double", "Двойная"),
            Self::Thick => language.text("Thick", "Толстая"),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Corners {
    #[default]
    Square,
    Rounded,
}
impl Corners {
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Square => language.text("Square", "Прямые"),
            Self::Rounded => language.text("Rounded", "Скруглённые"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub palette_ids: [String; 2],
    #[serde(default)]
    pub preset_ids: [Option<String>; 2],
    pub font: FontFace,
    pub font_size: u16,
    pub borders: BorderWeight,
    pub corners: Corners,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            palette_ids: [BRAND_PALETTE.into(), BRAND_PALETTE.into()],
            preset_ids: [Some(BRAND_PALETTE.into()), Some(BRAND_PALETTE.into())],
            font: FontFace::default(),
            font_size: 16,
            borders: BorderWeight::Single,
            corners: Corners::Square,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamedPalette {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub colors: BTreeMap<String, [u8; 3]>,
}

#[derive(Deserialize)]
pub struct BrandColor {
    pub token: String,
    pub hex: String,
    #[serde(default)]
    pub role_en: String,
    #[serde(default)]
    pub role_ru: String,
}
#[derive(Deserialize)]
struct BrandPalette {
    name: String,
    version: u32,
    colors: Vec<BrandColor>,
}
fn brand() -> &'static BrandPalette {
    static BRAND: OnceLock<BrandPalette> = OnceLock::new();
    BRAND.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/brand/molten-zharr.json"))
            .expect("Bundled brand palette is valid")
    })
}
pub fn color_roles() -> &'static [BrandColor] {
    &brand().colors
}
pub fn color_role_name(token: &str, language: Language) -> &str {
    match token {
        "background" => language.text("Background", "Фон"),
        "text" => language.text("Text", "Текст"),
        "button_text" => language.text("Button text", "Текст кнопок"),
        "active" => language.text("Active element", "Активный элемент"),
        "sidebar_title" => language.text("Sidebar title", "Заголовок меню"),
        "error" => language.text("Error", "Ошибка"),
        "inactive_file_border" => {
            language.text("Inactive file border", "Граница неактивного файла")
        }
        "inactive_panel_border" => {
            language.text("Inactive panel border", "Граница неактивной панели")
        }
        "gradient_dark" => language.text("Dark gradient", "Тёмный цвет градиента"),
        "folder_icon" => language.text("Folder icon", "Значок папки"),
        "selection_accent" => language.text("Selection accent", "Акцент выделения"),
        "confirm_background" => language.text("Confirmation background", "Фон подтверждения"),
        "help_heading" => language.text("Help heading", "Заголовок справки"),
        "folder_path" => language.text("Folder path", "Путь к папке"),
        "separator" => language.text("Separator", "Разделитель"),
        "selected_file_background" => {
            language.text("Selected file background", "Фон выбранного файла")
        }
        _ => token,
    }
}
pub fn parse_color(text: &str) -> Result<[u8; 3]> {
    let hex = text.trim().strip_prefix('#').unwrap_or(text.trim());
    ensure!(hex.len() == 6 && hex.is_ascii(), AppError::InvalidColor);
    let rgb = u32::from_str_radix(hex, 16).context(AppError::InvalidColor)?;
    Ok([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8])
}
pub fn color_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
pub fn validate_name(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty()
            && name.chars().count() <= 80
            && !name.chars().any(char::is_control),
        AppError::InvalidCatalogName
    );
    Ok(())
}
impl NamedPalette {
    pub fn color(&self, token: &str) -> [u8; 3] {
        self.colors.get(token).copied().unwrap_or([0, 0, 0])
    }
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        ensure!(self.colors.len() <= 64, "Palette contains too many colors");
        for role in color_roles() {
            ensure!(
                self.colors.contains_key(&role.token),
                "Palette is missing color: {}",
                role.token
            );
        }
        Ok(())
    }
    fn from_brand(source: BrandPalette) -> Result<Self> {
        ensure!(source.version == 2, "Unsupported brand palette version");
        let mut colors = BTreeMap::new();
        for role in source.colors {
            ensure!(
                !colors.contains_key(&role.token),
                "Duplicate palette color: {}",
                role.token
            );
            colors.insert(role.token, parse_color(&role.hex)?);
        }
        let palette = Self {
            id: BRAND_PALETTE.into(),
            name: source.name,
            colors,
        };
        palette.validate()?;
        Ok(palette)
    }
    pub fn import_json(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= 1_048_576, AppError::PaletteFileTooLarge);
        let value: serde_json::Value =
            serde_json::from_slice(bytes).context(AppError::InvalidPaletteJson)?;
        if value.get("format").is_some() {
            let document: PaletteDocument = serde_json::from_value(value)?;
            ensure!(
                document.format == "almavorn.palette" && document.version == 1,
                AppError::UnsupportedPaletteVersion
            );
            document.palette.validate()?;
            Ok(document.palette)
        } else {
            Self::from_brand(
                serde_json::from_value(value).context(AppError::UnsupportedPaletteFormat)?,
            )
        }
    }
    pub fn export_json(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(&PaletteDocument {
            format: "almavorn.palette".into(),
            version: 1,
            palette: self.clone(),
        })?)
    }
}
#[derive(Serialize, Deserialize)]
struct PaletteDocument {
    format: String,
    version: u32,
    palette: NamedPalette,
}

pub fn default_palettes() -> Vec<NamedPalette> {
    let molten = NamedPalette::from_brand(
        serde_json::from_str(include_str!("../assets/brand/molten-zharr.json"))
            .expect("Bundled palette JSON"),
    )
    .expect("Bundled palette colors");
    let mut amber = molten.clone();
    amber.id = "classic-amber".into();
    amber.name = "Classic Amber".into();
    for (key, color) in [
        ("background", [18, 22, 29]),
        ("text", [223, 228, 238]),
        ("active", [232, 158, 74]),
        ("inactive_panel_border", [138, 153, 175]),
        ("inactive_file_border", [138, 153, 175]),
        ("folder_path", [138, 153, 175]),
        ("selected_file_background", [49, 61, 78]),
        ("error", [255, 119, 135]),
    ] {
        amber.colors.insert(key.into(), color);
    }
    let mut violet = amber.clone();
    violet.id = "classic-violet".into();
    violet.name = "Classic Violet".into();
    violet.colors.insert("active".into(), [188, 125, 228]);
    let mut light = amber.clone();
    light.id = "light".into();
    light.name = "Light".into();
    for (key, color) in [
        ("background", [239, 242, 246]),
        ("text", [31, 39, 49]),
        ("active", [152, 78, 0]),
        ("folder_path", [85, 99, 116]),
        ("selected_file_background", [212, 225, 239]),
        ("error", [174, 37, 57]),
        ("button_text", [31, 39, 49]),
    ] {
        light.colors.insert(key.into(), color);
    }
    vec![molten, amber, violet, light]
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThemeStyle {
    pub palette_id: String,
    pub font: FontFace,
    pub font_size: u16,
    pub borders: BorderWeight,
    pub corners: Corners,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemePreset {
    pub id: String,
    pub name: String,
    pub style: ThemeStyle,
}
pub fn default_presets() -> Vec<ThemePreset> {
    default_palettes()
        .into_iter()
        .map(|p| ThemePreset {
            id: p.id.clone(),
            name: p.name,
            style: ThemeStyle {
                palette_id: p.id,
                font: FontFace::default(),
                font_size: 16,
                borders: BorderWeight::Single,
                corners: Corners::Square,
            },
        })
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProfilePreferences {
    pub language: Language,
    pub volume: f32,
    pub sorting_desk: bool,
    pub playlist_placement: Placement,
    pub player_placement: Placement,
    #[serde(default)]
    pub workspace: crate::workspace::WorkspaceLayout,
    pub themes: [Theme; 2],
    pub bindings: Vec<Binding>,
    pub appearance: Appearance,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettingsProfile {
    pub id: String,
    pub name: String,
    pub preferences: ProfilePreferences,
}
impl Settings {
    pub fn profile_preferences(&self) -> ProfilePreferences {
        ProfilePreferences {
            language: self.language,
            volume: self.volume,
            sorting_desk: self.sorting_desk,
            playlist_placement: self.playlist_placement,
            player_placement: self.player_placement,
            workspace: self.workspace.clone(),
            themes: self.themes.clone(),
            bindings: self.bindings.clone(),
            appearance: self.appearance.clone(),
        }
    }
    pub fn apply_preferences(&mut self, preferences: ProfilePreferences) {
        self.language = preferences.language;
        self.volume = preferences.volume.clamp(0.0, 1.0);
        self.sorting_desk = preferences.sorting_desk;
        self.playlist_placement = preferences.playlist_placement;
        self.player_placement = preferences.player_placement;
        self.workspace = preferences.workspace;
        self.workspace.normalize();
        self.themes = preferences.themes;
        self.bindings = preferences.bindings;
        self.appearance = preferences.appearance;
    }
    pub fn sync_active_profile(&mut self) {
        let preferences = self.profile_preferences();
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == self.active_profile)
        {
            profile.preferences = preferences;
        }
    }
    pub fn initialize_profiles(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(SettingsProfile {
                id: "default".into(),
                name: "Default".into(),
                preferences: self.profile_preferences(),
            });
            self.active_profile = "default".into();
        }
    }
    pub fn fresh_id(&mut self, prefix: &str) -> String {
        loop {
            let id = format!("{prefix}-{}", self.next_settings_id);
            self.next_settings_id = self.next_settings_id.wrapping_add(1);
            if !self.palettes.iter().any(|p| p.id == id)
                && !self.profiles.iter().any(|p| p.id == id)
                && !self.presets.iter().any(|p| p.id == id)
            {
                return id;
            }
        }
    }
    pub fn current_palette(&self) -> &NamedPalette {
        self.palettes
            .iter()
            .find(|p| p.id == self.appearance.palette_ids[self.mode.index()])
            .unwrap_or(&self.palettes[0])
    }
    pub fn current_style(&self) -> ThemeStyle {
        ThemeStyle {
            palette_id: self.current_palette().id.clone(),
            font: self.appearance.font,
            font_size: self.appearance.font_size,
            borders: self.appearance.borders,
            corners: self.appearance.corners,
        }
    }
    pub fn apply_style(&mut self, style: ThemeStyle) {
        self.appearance.preset_ids[self.mode.index()] = None;
        self.appearance.palette_ids[self.mode.index()] = style.palette_id;
        self.appearance.font = style.font;
        self.appearance.font_size = style.font_size;
        self.appearance.borders = style.borders;
        self.appearance.corners = style.corners;
    }
    pub fn current_preset(&self) -> Option<&ThemePreset> {
        let style = self.current_style();
        let selected = self.appearance.preset_ids[self.mode.index()].as_deref();
        self.presets
            .iter()
            .find(|preset| Some(preset.id.as_str()) == selected && preset.style == style)
            .or_else(|| self.presets.iter().find(|preset| preset.style == style))
    }
    pub fn apply_preset(&mut self, id: &str) -> Result<()> {
        let preset = self
            .presets
            .iter()
            .find(|preset| preset.id == id)
            .context("Preset no longer exists")?;
        let style = preset.style.clone();
        let id = preset.id.clone();
        self.apply_style(style);
        self.appearance.preset_ids[self.mode.index()] = Some(id);
        Ok(())
    }
    pub fn migrate_legacy_appearance(&mut self) {
        // Preserve customized legacy colors; the brand preset stays first in the catalog.
        for mode in Mode::ALL {
            let theme = &self.themes[mode.index()];
            let mut palette = default_palettes()[if theme.light { 3 } else { 1 }].clone();
            palette.id = format!("legacy-{}", mode.key());
            palette.name = format!("Legacy {}", mode.key());
            palette.colors.insert(
                "active".into(),
                if theme.light {
                    theme.accent.map(|c| c.saturating_sub(80))
                } else {
                    theme.accent
                },
            );
            self.appearance.palette_ids[mode.index()] = palette.id.clone();
            self.palettes.push(palette);
        }
        self.appearance.corners = Corners::Rounded;
    }
    pub fn validate_catalogs(&self) -> Result<()> {
        ensure!(
            !self.palettes.is_empty() && !self.profiles.is_empty() && !self.presets.is_empty(),
            "Settings catalogs cannot be empty"
        );
        let mut ids = HashSet::new();
        for palette in &self.palettes {
            palette.validate()?;
            ensure!(
                !palette.id.is_empty() && ids.insert(&palette.id),
                "Duplicate palette ID"
            );
        }
        let validate_appearance = |appearance: &Appearance| -> Result<()> {
            ensure!(
                (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&appearance.font_size),
                AppError::InvalidFontSize
            );
            ensure!(
                appearance.palette_ids.iter().all(|id| ids.contains(id)),
                "Settings refer to a missing palette"
            );
            Ok(())
        };
        validate_appearance(&self.appearance)?;
        let mut profile_ids = HashSet::new();
        for profile in &self.profiles {
            validate_name(&profile.name)?;
            ensure!(
                !profile.id.is_empty() && profile_ids.insert(&profile.id),
                "Duplicate profile ID"
            );
            validate_appearance(&profile.preferences.appearance)?;
        }
        ensure!(
            profile_ids.contains(&self.active_profile),
            "Active profile does not exist"
        );
        let mut preset_ids = HashSet::new();
        for preset in &self.presets {
            validate_name(&preset.name)?;
            ensure!(
                !preset.id.is_empty() && preset_ids.insert(&preset.id),
                "Duplicate preset ID"
            );
            ensure!(
                ids.contains(&preset.style.palette_id),
                "Preset refers to a missing palette"
            );
            ensure!(
                (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&preset.style.font_size),
                AppError::InvalidFontSize
            );
        }
        for appearance in std::iter::once(&self.appearance).chain(
            self.profiles
                .iter()
                .map(|profile| &profile.preferences.appearance),
        ) {
            ensure!(
                appearance
                    .preset_ids
                    .iter()
                    .flatten()
                    .all(|id| preset_ids.contains(id)),
                "Settings refer to a missing theme preset"
            );
        }
        Ok(())
    }
    pub fn remove_preset(&mut self, id: &str) -> Result<()> {
        ensure!(self.presets.len() > 1, AppError::LastPreset);
        ensure!(
            self.presets.iter().any(|preset| preset.id == id),
            "Preset no longer exists"
        );
        for appearance in std::iter::once(&mut self.appearance).chain(
            self.profiles
                .iter_mut()
                .map(|profile| &mut profile.preferences.appearance),
        ) {
            for selected in &mut appearance.preset_ids {
                if selected.as_deref() == Some(id) {
                    *selected = None;
                }
            }
        }
        self.presets.retain(|preset| preset.id != id);
        Ok(())
    }
    pub fn remove_palette(&mut self, id: &str) -> Result<()> {
        ensure!(self.palettes.len() > 1, AppError::LastPalette);
        let replacement = self
            .palettes
            .iter()
            .find(|p| p.id != id)
            .context(AppError::PaletteMissing)?
            .id
            .clone();
        let replace = |appearance: &mut Appearance| {
            for selected in &mut appearance.palette_ids {
                if selected == id {
                    *selected = replacement.clone();
                }
            }
        };
        replace(&mut self.appearance);
        for profile in &mut self.profiles {
            replace(&mut profile.preferences.appearance);
        }
        for preset in &mut self.presets {
            if preset.style.palette_id == id {
                preset.style.palette_id = replacement.clone();
            }
        }
        self.palettes.retain(|p| p.id != id);
        Ok(())
    }
}
