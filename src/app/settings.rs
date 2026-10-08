mod catalogs;
mod files;
mod items;

use super::{App, Dialog, Target, TextPurpose, bounded};
use crate::{
    input::{Key, KeyPress},
    model::{Language, Placement},
    preferences::{
        BorderWeight, Corners, FontFace, MAX_FONT_SIZE, MIN_FONT_SIZE, PlaybackTimeline, color_hex,
    },
};
use anyhow::Result;
pub(crate) use files::SettingsFileJob;
pub(crate) use items::SettingControl;
use ratatui::layout::Rect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsPage {
    #[default]
    General,
    Profiles,
    Themes,
    Palettes,
    Typography,
    Shortcuts,
}
impl SettingsPage {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Profiles,
        Self::Themes,
        Self::Palettes,
        Self::Typography,
        Self::Shortcuts,
    ];
    pub const SECTIONS: [Self; 4] = [Self::General, Self::Profiles, Self::Themes, Self::Shortcuts];
    pub const THEME_TABS: [Self; 3] = [Self::Themes, Self::Palettes, Self::Typography];

    pub fn section(self) -> Self {
        match self {
            Self::Palettes | Self::Typography => Self::Themes,
            page => page,
        }
    }

    pub fn tab_name(self, language: Language) -> &'static str {
        if self == Self::Themes {
            language.text("Presets", "Пресеты")
        } else {
            self.name(language)
        }
    }
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::General => language.text("General", "Общие"),
            Self::Profiles => language.text("Profiles", "Профили"),
            Self::Themes => language.text("Themes", "Темы"),
            Self::Palettes => language.text("Palettes", "Палитры"),
            Self::Typography => language.text("Font and geometry", "Шрифт и геометрия"),
            Self::Shortcuts => language.text("Shortcuts", "Клавиши"),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsFocus {
    #[default]
    Menu,
    Subtabs,
    Parameters,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct SettingsView {
    pub page: SettingsPage,
    pub focus: SettingsFocus,
    pub selected: usize,
    pub profile: usize,
    pub palette: usize,
    pub preset: usize,
    pub offset: usize,
    pub menu_area: Rect,
    pub subtab_area: Rect,
    pub parameters_area: Rect,
    pub binding_modifiers: [bool; 3],
    pub binding_offset: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsCatalog {
    Profile,
    Palette,
    Preset,
}
#[derive(Clone, Debug)]
pub enum SettingsEdit {
    CreateProfile,
    RenameProfile(String),
    CreatePalette,
    RenamePalette(String),
    PaletteColor { id: String, token: String },
    ImportPalette,
    ExportPalette(String),
    CreatePreset,
    RenamePreset(String),
    FontSize,
}
impl SettingsEdit {
    pub fn title(&self, language: Language) -> &'static str {
        match self {
            Self::CreateProfile => language.text(
                "New profile from current settings",
                "Новый профиль из текущих настроек",
            ),
            Self::RenameProfile(_) => language.text("Rename profile", "Переименовать профиль"),
            Self::CreatePalette => language.text(
                "New palette from selected colors",
                "Новая палитра из выбранных цветов",
            ),
            Self::RenamePalette(_) => language.text("Rename palette", "Переименовать палитру"),
            Self::PaletteColor { .. } => language.text("Color - #RRGGBB", "Цвет - #RRGGBB"),
            Self::ImportPalette => language.text(
                "Import palette - JSON file path",
                "Импорт палитры - путь к JSON",
            ),
            Self::ExportPalette(_) => language.text(
                "Export palette - new JSON file path",
                "Экспорт палитры - путь к новому JSON",
            ),
            Self::CreatePreset => language.text(
                "New preset from current appearance",
                "Новый пресет из текущего оформления",
            ),
            Self::RenamePreset(_) => {
                language.text("Rename theme preset", "Переименовать пресет темы")
            }
            Self::FontSize => {
                language.text("Font size - 10-32 pixels", "Размер шрифта - 10-32 пикселя")
            }
        }
    }
}

fn cycle<T: Copy + PartialEq>(current: T, all: &[T], direction: i64) -> T {
    let position = all.iter().position(|value| *value == current).unwrap_or(0) as i64;
    all[(position + direction).rem_euclid(all.len() as i64) as usize]
}
fn cycle_index(current: usize, length: usize, direction: i64) -> usize {
    (current as i64 + direction).rem_euclid(length.max(1) as i64) as usize
}

impl App {
    pub fn settings_page(&self) -> SettingsPage {
        self.view.settings.page
    }
    pub fn settings_focus(&self) -> SettingsFocus {
        self.view.settings.focus
    }
    pub fn open_settings_page(&mut self, page: SettingsPage) {
        if !matches!(self.view.dialog, Some(Dialog::Settings { .. })) {
            self.message(
                self.text(
                    "Changes save automatically to the active profile",
                    "Изменения автоматически сохраняются в активный профиль",
                )
                .into(),
            );
        }
        self.view.settings.page = page;
        self.view.settings.selected = 0;
        self.view.settings.offset = 0;
        self.view.settings.focus = SettingsFocus::Menu;
        self.show_settings();
    }
    pub(super) fn show_settings(&mut self) {
        self.view.dialog = Some(Dialog::Settings {
            selected: self.view.settings.selected,
        });
    }
    pub(super) fn select_settings_tab(&mut self, page: SettingsPage) {
        if SettingsPage::THEME_TABS.contains(&page) {
            self.open_settings_page(page);
            self.view.settings.focus = SettingsFocus::Subtabs;
        }
    }
    pub(super) fn hover_settings(&mut self) {
        if !matches!(self.view.dialog, Some(Dialog::Settings { .. }))
            || self.view.workspace.gesture.is_some()
        {
            return;
        }
        let target = self
            .view
            .hits
            .iter()
            .rev()
            .find(|hit| hit.enabled && hit.area.contains(self.view.pointer))
            .map(|hit| hit.target.clone());
        match target {
            Some(Target::SettingsPage(page))
                if self.view.settings.menu_area.contains(self.view.pointer) =>
            {
                if self.view.settings.page.section() != page {
                    self.open_settings_page(page);
                }
                self.view.settings.focus = SettingsFocus::Menu;
            }
            Some(Target::SettingsTab(page))
                if self.view.settings.subtab_area.contains(self.view.pointer) =>
            {
                if self.view.settings.page != page {
                    self.select_settings_tab(page);
                }
                self.view.settings.focus = SettingsFocus::Subtabs;
            }
            _ => {}
        }
    }
    pub(super) fn close_dialog(&mut self) -> bool {
        match self.view.dialog.take() {
            Some(
                Dialog::Text(super::TextDialog {
                    purpose: TextPurpose::Settings(_),
                    ..
                })
                | Dialog::ConfirmSettings { .. }
                | Dialog::SettingsHelp { .. }
                | Dialog::CaptureBinding { .. },
            ) => {
                self.show_settings();
                true
            }
            Some(Dialog::Browser(browser)) => {
                self.cancel_browser();
                self.view.dialog = browser.playlist_name.map(Dialog::Text);
                true
            }
            Some(_) => true,
            None => false,
        }
    }
    pub(super) fn settings_scroll(&mut self, direction: i64) {
        if self.view.settings.focus != SettingsFocus::Parameters {
            let subtabs = self.view.settings.focus == SettingsFocus::Subtabs;
            let pages = if subtabs {
                SettingsPage::THEME_TABS.as_slice()
            } else {
                SettingsPage::SECTIONS.as_slice()
            };
            let current = if subtabs {
                self.view.settings.page
            } else {
                self.view.settings.page.section()
            };
            let index = pages.iter().position(|page| *page == current).unwrap_or(0);
            let page = pages[bounded(index, direction, pages.len())];
            if subtabs {
                self.select_settings_tab(page);
            } else {
                self.open_settings_page(page);
            }
        } else {
            self.view.settings.selected = bounded(
                self.view.settings.selected,
                direction,
                self.settings_rows().len(),
            );
            self.show_settings();
        }
    }
    pub(super) fn settings_key(&mut self, key: KeyPress) -> Result<()> {
        let key = key.normalized();
        match key.key {
            Key::Tab => {
                let focuses = if self.view.settings.page.section() == SettingsPage::Themes {
                    &[
                        SettingsFocus::Menu,
                        SettingsFocus::Subtabs,
                        SettingsFocus::Parameters,
                    ][..]
                } else {
                    &[SettingsFocus::Menu, SettingsFocus::Parameters][..]
                };
                self.view.settings.focus = cycle(
                    self.view.settings.focus,
                    focuses,
                    if key.shift { -1 } else { 1 },
                );
            }
            Key::Up => self.settings_scroll(-1),
            Key::Down => self.settings_scroll(1),
            Key::Home => self.settings_scroll(i64::MIN),
            Key::End => self.settings_scroll(i64::MAX),
            Key::PageUp => self.settings_scroll(-5),
            Key::PageDown => self.settings_scroll(5),
            Key::Right if self.view.settings.focus == SettingsFocus::Menu => {
                self.view.settings.focus =
                    if self.view.settings.page.section() == SettingsPage::Themes {
                        SettingsFocus::Subtabs
                    } else {
                        SettingsFocus::Parameters
                    };
            }
            Key::Left | Key::Right if self.view.settings.focus == SettingsFocus::Subtabs => {
                self.settings_scroll(if key.key == Key::Left { -1 } else { 1 });
            }
            Key::Left | Key::Right if self.view.settings.focus == SettingsFocus::Parameters => {
                let direction = if key.key == Key::Left { -1 } else { 1 };
                self.adjust_setting(self.view.settings.selected, direction)?;
            }
            Key::Enter | Key::Char(' ') => {
                if self.view.settings.focus != SettingsFocus::Parameters {
                    self.view.settings.focus = if self.view.settings.focus == SettingsFocus::Menu
                        && self.view.settings.page.section() == SettingsPage::Themes
                    {
                        SettingsFocus::Subtabs
                    } else {
                        SettingsFocus::Parameters
                    };
                } else {
                    self.setting(self.view.settings.selected)?;
                }
            }
            Key::Insert => self.settings_catalog_shortcut(false)?,
            Key::F(3) => self.settings_catalog_shortcut(true)?,
            Key::Delete => self.request_settings_delete()?,
            Key::F(1) => self.settings_help(self.view.settings.selected),
            Key::Char('i')
                if key.ctrl
                    && self.view.settings.page == SettingsPage::Palettes
                    && !self.settings_file_busy() =>
            {
                self.settings_text(SettingsEdit::ImportPalette, String::new());
            }
            Key::Char('e')
                if key.ctrl
                    && self.view.settings.page == SettingsPage::Palettes
                    && !self.settings_file_busy() =>
            {
                self.export_selected_palette();
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn select_setting(&mut self, row: usize) {
        self.view.settings.focus = SettingsFocus::Parameters;
        self.view.settings.selected = row.min(self.settings_rows().len().saturating_sub(1));
        self.show_settings();
    }
    pub(super) fn settings_help(&mut self, row: usize) {
        if let Some(item) = self.settings_rows().get(row) {
            self.view.dialog = Some(Dialog::SettingsHelp {
                title: item.label.clone(),
                description: item.description.clone(),
                offset: 0,
            });
        }
    }
    pub(super) fn setting(&mut self, row: usize) -> Result<()> {
        let Some(item) = self.settings_rows().get(row).cloned() else {
            return Ok(());
        };
        if !item.enabled {
            return Ok(());
        }
        self.select_setting(row);
        match item.control {
            SettingControl::ProfileCreate => self.settings_text(
                SettingsEdit::CreateProfile,
                self.text("New profile", "Новый профиль").into(),
            ),
            SettingControl::ProfileRename => {
                let p = &self.settings.profiles[self
                    .view
                    .settings
                    .profile
                    .min(self.settings.profiles.len() - 1)];
                self.settings_text(SettingsEdit::RenameProfile(p.id.clone()), p.name.clone());
            }
            SettingControl::ProfileApply => self.apply_selected_profile()?,
            SettingControl::PaletteCreate => self.settings_text(
                SettingsEdit::CreatePalette,
                self.text("New palette", "Новая палитра").into(),
            ),
            SettingControl::PaletteRename => {
                let p = self.selected_settings_palette();
                self.settings_text(SettingsEdit::RenamePalette(p.id.clone()), p.name.clone());
            }
            SettingControl::PaletteColor(token) => {
                let p = self.selected_settings_palette();
                self.settings_text(
                    SettingsEdit::PaletteColor {
                        id: p.id.clone(),
                        token: token.clone(),
                    },
                    color_hex(p.color(&token)),
                );
            }
            SettingControl::PaletteApply => {
                self.settings.appearance.palette_ids[self.settings.mode.index()] =
                    self.selected_settings_palette().id.clone();
                self.save_settings()?;
            }
            SettingControl::PaletteImport => {
                self.settings_text(SettingsEdit::ImportPalette, String::new())
            }
            SettingControl::PaletteExport => self.export_selected_palette(),
            SettingControl::PresetCreate => self.settings_text(
                SettingsEdit::CreatePreset,
                self.text("New preset", "Новый пресет").into(),
            ),
            SettingControl::PresetRename => {
                let p = &self.settings.presets[self.selected_settings_preset_index()];
                self.settings_text(SettingsEdit::RenamePreset(p.id.clone()), p.name.clone());
            }
            SettingControl::PresetUpdate => {
                let style = self.settings.current_style();
                let index = self.selected_settings_preset_index();
                self.settings.presets[index].style = style;
                self.settings.appearance.preset_ids[self.settings.mode.index()] =
                    Some(self.settings.presets[index].id.clone());
                self.save_settings()?;
            }
            SettingControl::DeleteCatalog => self.request_settings_delete()?,
            SettingControl::FontSize => self.settings_text(
                SettingsEdit::FontSize,
                self.settings.appearance.font_size.to_string(),
            ),
            SettingControl::Binding(index) => {
                self.view.settings.binding_modifiers = [false; 3];
                self.view.settings.binding_offset = 0;
                self.view.dialog = Some(Dialog::CaptureBinding { index });
            }
            SettingControl::ResetBindings => {
                self.settings.bindings = crate::input::default_bindings();
                self.save_settings()?;
            }
            _ => self.adjust_setting(row, 1)?,
        }
        Ok(())
    }
    pub(super) fn adjust_setting(&mut self, row: usize, direction: i64) -> Result<()> {
        let Some(item) = self.settings_rows().get(row).cloned() else {
            return Ok(());
        };
        if !item.enabled || !item.adjustable {
            return Ok(());
        }
        self.select_setting(row);
        match item.control {
            SettingControl::Language => {
                self.settings.language = cycle(
                    self.settings.language,
                    &[Language::English, Language::Russian],
                    direction,
                )
            }
            SettingControl::Volume => {
                return self.set_settings_volume(self.settings.volume + direction as f32 * 0.05);
            }
            SettingControl::Desk => self.settings.sorting_desk = !self.settings.sorting_desk,
            SettingControl::ButtonShortcuts => {
                self.settings.appearance.show_button_shortcuts =
                    !self.settings.appearance.show_button_shortcuts;
            }
            SettingControl::PlaybackTimeline => {
                return self.set_playback_timeline(cycle(
                    self.settings.appearance.playback_timeline,
                    &[PlaybackTimeline::Waveform, PlaybackTimeline::Progress],
                    direction,
                ));
            }
            SettingControl::PlaylistPlacement => {
                self.settings.workspace.root = None;
                self.settings.playlist_placement = cycle(
                    self.settings.playlist_placement,
                    &[
                        Placement::Left,
                        Placement::Right,
                        Placement::Top,
                        Placement::Bottom,
                    ],
                    direction,
                )
            }
            SettingControl::PlayerPlacement => {
                self.settings.workspace.root = None;
                self.settings.player_placement = cycle(
                    self.settings.player_placement,
                    &[
                        Placement::Left,
                        Placement::Right,
                        Placement::Top,
                        Placement::Bottom,
                    ],
                    direction,
                )
            }
            SettingControl::ProfilePick => {
                self.view.settings.profile = cycle_index(
                    self.view.settings.profile,
                    self.settings.profiles.len(),
                    direction,
                );
                return Ok(());
            }
            SettingControl::PalettePick => {
                self.view.settings.palette = cycle_index(
                    self.view.settings.palette,
                    self.settings.palettes.len(),
                    direction,
                );
                return Ok(());
            }
            SettingControl::PresetPick => {
                self.view.settings.preset = cycle_index(
                    self.selected_settings_preset_index(),
                    self.settings.presets.len(),
                    direction,
                );
                let id = self.settings.presets[self.view.settings.preset].id.clone();
                self.settings.apply_preset(&id)?;
            }
            SettingControl::ModePalette(mode) => {
                let selected = self
                    .settings
                    .palettes
                    .iter()
                    .position(|p| p.id == self.settings.appearance.palette_ids[mode.index()])
                    .unwrap_or(0);
                self.settings.appearance.palette_ids[mode.index()] = self.settings.palettes
                    [cycle_index(selected, self.settings.palettes.len(), direction)]
                .id
                .clone();
            }
            SettingControl::Font => {
                self.settings.appearance.font =
                    cycle(self.settings.appearance.font, &FontFace::ALL, direction)
            }
            SettingControl::FontSize => {
                self.settings.appearance.font_size = (i64::from(self.settings.appearance.font_size)
                    + direction)
                    .clamp(i64::from(MIN_FONT_SIZE), i64::from(MAX_FONT_SIZE))
                    as u16
            }
            SettingControl::Borders => {
                self.settings.appearance.borders = cycle(
                    self.settings.appearance.borders,
                    &BorderWeight::ALL,
                    direction,
                )
            }
            SettingControl::Corners => {
                self.settings.appearance.corners = cycle(
                    self.settings.appearance.corners,
                    &[Corners::Square, Corners::Rounded],
                    direction,
                )
            }
            _ => return Ok(()),
        }
        self.save_settings()
    }
    pub(super) fn set_settings_volume(&mut self, volume: f32) -> Result<()> {
        self.settings.volume = volume.clamp(0.0, 1.0);
        if let Some(audio) = &self.playback.audio {
            audio.volume(self.settings.volume);
        }
        self.save_settings()
    }
    fn settings_text(&mut self, edit: SettingsEdit, value: String) {
        self.text_dialog(TextPurpose::Settings(edit), value);
    }
    fn export_selected_palette(&mut self) {
        let p = self.selected_settings_palette();
        let path = self
            .store
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{}.json", p.id));
        self.settings_text(
            SettingsEdit::ExportPalette(p.id.clone()),
            path.to_string_lossy().into(),
        );
    }
    pub(crate) fn selected_settings_palette(&self) -> &crate::preferences::NamedPalette {
        &self.settings.palettes[self
            .view
            .settings
            .palette
            .min(self.settings.palettes.len() - 1)]
    }
    pub(crate) fn selected_settings_preset_index(&self) -> usize {
        self.settings
            .current_preset()
            .and_then(|current| {
                self.settings
                    .presets
                    .iter()
                    .position(|preset| preset.id == current.id)
            })
            .unwrap_or_else(|| {
                self.view
                    .settings
                    .preset
                    .min(self.settings.presets.len() - 1)
            })
    }
}
