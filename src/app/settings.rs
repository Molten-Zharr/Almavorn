mod catalogs;
mod files;
mod items;
mod persistence;

use super::{App, Dialog, Target, TextPurpose, bounded};
use crate::{
    errors::AppError,
    input::{Key, KeyPress},
    model::{Language, Placement},
    preferences::{
        BorderWeight, Corners, FontFace, MAX_FONT_SIZE, MIN_FONT_SIZE, PlaybackTimeline, color_hex,
    },
};
use anyhow::Result;
pub(crate) use files::SettingsFileJob;
pub(crate) use items::{SettingControl, SettingGroup};
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
    pub menu_selected: usize,
    pub themes_expanded: bool,
    pub menu_area: Rect,
    pub parameters_area: Rect,
    pub binding_modifiers: [bool; 3],
    pub binding_offset: usize,
    pub binding_global_focus: Option<usize>,
    pub binding_error: Option<String>,
    pub file_directory: Option<std::path::PathBuf>,
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
    PaletteColor {
        id: String,
        token: String,
    },
    ExportPalette {
        id: String,
        directory: std::path::PathBuf,
    },
    CreatePreset,
    RenamePreset(String),
    FontSize,
    ArtworkPath,
    GlobalShortcut(crate::input::Action),
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
            Self::ExportPalette { .. } => {
                language.text("Export palette - file name", "Экспорт палитры - имя файла")
            }
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
            Self::ArtworkPath => language.text("Shared image - full path", "Общая картинка - полный путь"),
            Self::GlobalShortcut(_) => {
                language.text("Global shortcut · XKB name", "Глобальная клавиша · имя XKB")
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
    pub(super) fn capture_binding_key(&mut self, index: usize, key: KeyPress) -> Result<()> {
        if key.key == Key::Tab && !key.ctrl && !key.alt {
            self.view.settings.binding_global_focus =
                match (self.view.settings.binding_global_focus, key.shift) {
                    (None, false) | (Some(1), true) => Some(0),
                    (Some(0), false) | (None, true) => Some(1),
                    _ => None,
                };
            return Ok(());
        }
        if let Some(selected) = self.view.settings.binding_global_focus {
            match key.key {
                Key::Left | Key::Up => self.view.settings.binding_global_focus = Some(0),
                Key::Right | Key::Down => self.view.settings.binding_global_focus = Some(1),
                Key::Enter | Key::Char(' ') => {
                    return self.target(
                        if selected == 0 {
                            Target::BindingGlobal(index)
                        } else {
                            Target::BindingGlobalEdit(index)
                        },
                        false,
                    );
                }
                _ => {}
            }
            return Ok(());
        }
        let key = key.normalized();
        self.view.settings.binding_error = None;
        let error = if matches!(
            key.key,
            Key::Tab
                | Key::Up
                | Key::Down
                | Key::Left
                | Key::Right
                | Key::Home
                | Key::End
                | Key::PageUp
                | Key::PageDown
        ) && !key.ctrl
            && !key.alt
        {
            Some(
                AppError::ReservedShortcut
                    .message(self.settings.preferences.language)
                    .to_owned(),
            )
        } else if let Some((_, binding)) = self
            .settings
            .preferences
            .bindings
            .iter()
            .enumerate()
            .find(|(other, binding)| *other != index && binding.key == key)
        {
            Some(format!(
                "{} {} \"{}\". {}",
                key.label(),
                self.text("is assigned to", "уже назначено действию"),
                binding.action.name(self.settings.preferences.language),
                self.text("Choose another shortcut.", "Выберите другое сочетание.")
            ))
        } else {
            None
        };
        if let Some(error) = error {
            self.view.settings.binding_error = Some(error.clone());
            anyhow::bail!("{error}");
        }
        self.settings.preferences.bindings[index].key = key;
        self.save_settings()?;
        self.view.settings.page = SettingsPage::Shortcuts;
        self.sync_settings_menu();
        self.view.settings.selected = self
            .settings_rows()
            .iter()
            .position(|row| matches!(row.control, SettingControl::Binding(value) if value == index))
            .unwrap_or(0);
        self.view.settings.focus = SettingsFocus::Parameters;
        self.show_settings();
        self.message(format!(
            "{}: {}",
            self.text("Shortcut saved", "Сочетание сохранено"),
            key.label()
        ));
        Ok(())
    }

    pub fn settings_page(&self) -> SettingsPage {
        self.view.settings.page
    }
    pub fn settings_focus(&self) -> SettingsFocus {
        self.view.settings.focus
    }
    pub(super) fn sync_settings_menu(&mut self) {
        let page = self.view.settings.page;
        if page.section() == SettingsPage::Themes {
            self.view.settings.themes_expanded = true;
        }
        self.view.settings.menu_selected = self
            .settings_menu_targets()
            .iter()
            .position(|target| {
                matches!(target,
                    Target::SettingsPage(value) | Target::SettingsTab(value) if *value == page)
            })
            .unwrap_or(0);
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
        self.sync_settings_menu();
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
    pub(crate) fn settings_menu_targets(&self) -> Vec<Target> {
        let mut items = vec![
            Target::SettingsPage(SettingsPage::General),
            Target::SettingsPage(SettingsPage::Profiles),
            Target::SettingsThemes,
        ];
        if self.view.settings.themes_expanded {
            items.extend(SettingsPage::THEME_TABS.map(Target::SettingsTab));
        }
        items.push(Target::SettingsPage(SettingsPage::Shortcuts));
        items
    }
    fn focus_settings_themes(&mut self) {
        self.view.settings.menu_selected = self
            .settings_menu_targets()
            .iter()
            .position(|target| matches!(target, Target::SettingsThemes))
            .unwrap_or(0);
        self.view.settings.focus = SettingsFocus::Menu;
    }
    pub(super) fn toggle_settings_themes(&mut self) {
        let expanded = !self.view.settings.themes_expanded;
        if expanded && self.view.settings.page.section() != SettingsPage::Themes {
            self.open_settings_page(SettingsPage::Themes);
        }
        self.view.settings.themes_expanded = expanded;
        self.focus_settings_themes();
        self.show_settings();
    }
    fn select_settings_menu(&mut self, index: usize) {
        let targets = self.settings_menu_targets();
        let Some(target) = targets.get(index) else {
            return;
        };
        match target {
            Target::SettingsPage(page) | Target::SettingsTab(page) => {
                if self.view.settings.page != *page {
                    self.open_settings_page(*page);
                }
                self.view.settings.focus = if matches!(target, Target::SettingsTab(_)) {
                    SettingsFocus::Subtabs
                } else {
                    SettingsFocus::Menu
                };
                self.view.settings.menu_selected = index;
            }
            Target::SettingsThemes => self.focus_settings_themes(),
            _ => {}
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
        if self.view.settings.menu_area.contains(self.view.pointer) {
            let index = self.settings_menu_targets().iter().position(|item| {
                match (item, target.as_ref()) {
                    (Target::SettingsThemes, Some(Target::SettingsThemes)) => true,
                    (Target::SettingsPage(a), Some(Target::SettingsPage(b)))
                    | (Target::SettingsTab(a), Some(Target::SettingsTab(b))) => a == b,
                    _ => false,
                }
            });
            if let Some(index) = index {
                self.select_settings_menu(index);
            }
        }
    }
    pub(super) fn close_dialog(&mut self) -> bool {
        if self.mods_close() { return true; }
        if self.services_close() { return true; }
        if self.downloads_close() { return true; }
        if self.form_close() || self.lyrics_close() { return true; }
        if self.close_tags() {
            return true;
        }
        match self.view.dialog.take() {
            Some(Dialog::Text(super::TextDialog {
                purpose:
                    TextPurpose::ComposeSelection {
                        sources,
                        entries,
                        available,
                    },
                ..
            })) => {
                self.view.dialog = Some(Dialog::ComposeTracks {
                    sources,
                    selected: 0,
                    entries: available,
                    included: entries.into_iter().collect(),
                });
                true
            }
            Some(
                Dialog::Text(super::TextDialog {
                    purpose: TextPurpose::Settings(_),
                    ..
                })
                | Dialog::ConfirmSettings { .. }
                | Dialog::ConfirmResetBindings { .. }
                | Dialog::SettingsHelp { .. }
                | Dialog::CopyableHelp { .. }
                | Dialog::CaptureBinding { .. },
            ) => {
                self.show_settings();
                true
            }
            Some(Dialog::Browser(browser)) => {
                self.cancel_browser();
                if browser.palette_file.is_some() {
                    self.show_settings();
                } else {
                    self.view.dialog = browser.playlist_name.map(Dialog::Text);
                }
                true
            }
            Some(_) => true,
            None => false,
        }
    }
    pub(super) fn settings_scroll(&mut self, direction: i64) {
        if self.view.settings.focus != SettingsFocus::Parameters {
            let index = bounded(
                self.view.settings.menu_selected,
                direction,
                self.settings_menu_targets().len(),
            );
            self.select_settings_menu(index);
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
                self.view.settings.focus = if self.view.settings.focus == SettingsFocus::Parameters
                {
                    SettingsFocus::Menu
                } else {
                    SettingsFocus::Parameters
                };
            }
            Key::Up => self.settings_scroll(-1),
            Key::Down => self.settings_scroll(1),
            Key::Home => self.settings_scroll(i64::MIN),
            Key::End => self.settings_scroll(i64::MAX),
            Key::PageUp => self.settings_scroll(-5),
            Key::PageDown => self.settings_scroll(5),
            Key::Right if self.view.settings.focus != SettingsFocus::Parameters => {
                if matches!(
                    self.settings_menu_targets()
                        .get(self.view.settings.menu_selected),
                    Some(Target::SettingsThemes)
                ) {
                    if self.view.settings.themes_expanded {
                        self.select_settings_menu(self.view.settings.menu_selected + 1);
                    } else {
                        self.toggle_settings_themes();
                    }
                } else {
                    self.view.settings.focus = SettingsFocus::Parameters;
                }
            }
            Key::Left if self.view.settings.focus != SettingsFocus::Parameters => {
                match self
                    .settings_menu_targets()
                    .get(self.view.settings.menu_selected)
                {
                    Some(Target::SettingsTab(_)) => self.focus_settings_themes(),
                    Some(Target::SettingsThemes) if self.view.settings.themes_expanded => {
                        self.toggle_settings_themes();
                    }
                    _ => {}
                }
            }
            Key::Left | Key::Right if self.view.settings.focus == SettingsFocus::Parameters => {
                let direction = if key.key == Key::Left { -1 } else { 1 };
                self.adjust_setting(self.view.settings.selected, direction)?;
            }
            Key::Enter | Key::Char(' ') => {
                if self.view.settings.focus != SettingsFocus::Parameters {
                    if matches!(
                        self.settings_menu_targets()
                            .get(self.view.settings.menu_selected),
                        Some(Target::SettingsThemes)
                    ) {
                        self.toggle_settings_themes();
                    } else {
                        self.view.settings.focus = SettingsFocus::Parameters;
                    }
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
                self.choose_palette_import();
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
                self.settings.preferences.appearance.palette_ids[self.settings.mode.index()] =
                    self.selected_settings_palette().id.clone();
                self.save_settings()?;
            }
            SettingControl::PaletteImport => self.choose_palette_import(),
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
                self.settings.preferences.appearance.preset_ids[self.settings.mode.index()] =
                    Some(self.settings.presets[index].id.clone());
                self.save_settings()?;
            }
            SettingControl::DeleteCatalog => self.request_settings_delete()?,
            SettingControl::FontSize => self.settings_text(
                SettingsEdit::FontSize,
                self.settings.preferences.appearance.font_size.to_string(),
            ),
            SettingControl::ArtworkPath => self.settings_text(
                SettingsEdit::ArtworkPath,
                self.settings.preferences.artwork.path.as_ref().map_or_else(String::new, |path| path.display().to_string()),
            ),
            SettingControl::Binding(index) => {
                self.view.settings.binding_modifiers = [false; 3];
                self.view.settings.binding_offset = 0;
                self.view.settings.binding_global_focus = None;
                self.view.settings.binding_error = None;
                self.view.dialog = Some(Dialog::CaptureBinding { index });
            }
            SettingControl::ResetBindings => {
                self.view.dialog = Some(Dialog::ConfirmResetBindings { confirm: false });
            }
            SettingControl::NiriBindings => self.show_niri_bindings()?,
            SettingControl::SystemMedia => self.show_system_media(),
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
                self.settings.preferences.language = cycle(
                    self.settings.preferences.language,
                    &[Language::English, Language::Russian],
                    direction,
                )
            }
            SettingControl::Volume => {
                return self.set_volume(self.settings.preferences.volume + direction as f32 * 0.05);
            }
            SettingControl::Desk => {
                self.settings.preferences.sorting_desk = !self.settings.preferences.sorting_desk
            }
            SettingControl::Visualizer => return self.cycle_visualizer(direction),
            SettingControl::AnimationBrightness => {
                self.settings.preferences.appearance.animation_brightness =
                    (i128::from(self.settings.preferences.appearance.animation_brightness)
                        + i128::from(direction) * 5)
                        .clamp(10, 70) as u8;
            }
            SettingControl::ButtonShortcuts => {
                self.settings.preferences.appearance.show_button_shortcuts =
                    !self.settings.preferences.appearance.show_button_shortcuts;
            }
            SettingControl::PlayerSpecialButtons => {
                self.settings.preferences.appearance.show_player_special_buttons = !self.settings.preferences.appearance.show_player_special_buttons;
                self.cancel_workspace_drag();
            }
            SettingControl::ExpandedToolbar => {
                self.settings.preferences.appearance.expanded_toolbar = !self.settings.preferences.appearance.expanded_toolbar;
                self.view.toolbar_selected = None;
            }
            SettingControl::ArtworkVisible => {
                self.settings.preferences.artwork.visible = !self.settings.preferences.artwork.visible;
            }
            SettingControl::ArtworkSize => {
                self.settings.preferences.artwork.size = cycle(
                    self.settings.preferences.artwork.size,
                    &crate::artwork::ArtworkSize::ALL,
                    direction,
                );
                self.cancel_workspace_drag();
            }
            SettingControl::ArtworkShared => {
                self.settings.preferences.artwork.shared = !self.settings.preferences.artwork.shared;
            }
            SettingControl::PlaybackTimeline => {
                return self.set_playback_timeline(cycle(
                    self.settings.preferences.appearance.playback_timeline,
                    &[PlaybackTimeline::Waveform, PlaybackTimeline::Progress],
                    direction,
                ));
            }
            SettingControl::PlaylistPlacement => {
                self.settings.preferences.workspace.root = None;
                self.settings.preferences.playlist_placement = cycle(
                    self.settings.preferences.playlist_placement,
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
                self.settings.preferences.workspace.root = None;
                self.settings.preferences.player_placement = cycle(
                    self.settings.preferences.player_placement,
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
                    .position(|p| {
                        p.id == self.settings.preferences.appearance.palette_ids[mode.index()]
                    })
                    .unwrap_or(0);
                self.settings.preferences.appearance.palette_ids[mode.index()] = self
                    .settings
                    .palettes[cycle_index(selected, self.settings.palettes.len(), direction)]
                .id
                .clone();
            }
            SettingControl::Font => {
                self.settings.preferences.appearance.font = cycle(
                    self.settings.preferences.appearance.font,
                    &FontFace::ALL,
                    direction,
                )
            }
            SettingControl::FontSize => {
                self.settings.preferences.appearance.font_size =
                    (i64::from(self.settings.preferences.appearance.font_size) + direction)
                        .clamp(i64::from(MIN_FONT_SIZE), i64::from(MAX_FONT_SIZE))
                        as u16
            }
            SettingControl::Borders => {
                self.settings.preferences.appearance.borders = cycle(
                    self.settings.preferences.appearance.borders,
                    &BorderWeight::ALL,
                    direction,
                )
            }
            SettingControl::Corners => {
                self.settings.preferences.appearance.corners = cycle(
                    self.settings.preferences.appearance.corners,
                    &[Corners::Square, Corners::Rounded],
                    direction,
                )
            }
            _ => return Ok(()),
        }
        self.save_settings()
    }
    fn settings_text(&mut self, edit: SettingsEdit, value: String) {
        self.text_dialog(TextPurpose::Settings(edit), value);
    }
    fn export_selected_palette(&mut self) {
        let p = self.selected_settings_palette();
        self.show_palette_browser(
            super::browser::PaletteFilePicker::Export {
                id: p.id.clone(),
                filename: format!("{}.json", p.id),
            },
            None,
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
