use super::{App, SettingsPage};
use crate::{
    model::Mode,
    preferences::{BorderWeight, color_hex, color_role_name, color_roles},
};

#[derive(Clone, Debug)]
pub(crate) enum SettingControl {
    Language,
    Volume,
    Desk,
    PlaylistPlacement,
    PlayerPlacement,
    PlaybackTimeline,
    ButtonShortcuts,
    ProfilePick,
    ProfileCreate,
    ProfileApply,
    ProfileRename,
    PresetPick,
    PresetCreate,
    PresetRename,
    PresetUpdate,
    ModePalette(Mode),
    PalettePick,
    PaletteApply,
    PaletteCreate,
    PaletteRename,
    PaletteColor(String),
    PaletteImport,
    PaletteExport,
    Font,
    FontSize,
    Borders,
    Corners,
    Binding(usize),
    ResetBindings,
    DeleteCatalog,
}
#[derive(Clone, Debug)]
pub(crate) struct SettingRow {
    pub label: String,
    pub value: String,
    pub description: String,
    pub control: SettingControl,
    pub adjustable: bool,
    pub enabled: bool,
    pub color: Option<[u8; 3]>,
}
impl App {
    pub(crate) fn settings_rows(&self) -> Vec<SettingRow> {
        let lang = self.settings.language;
        let make = |label: &str,
                    value: String,
                    description: &str,
                    control: SettingControl,
                    adjustable: bool| SettingRow {
            label: label.into(),
            value,
            description: description.into(),
            control,
            adjustable,
            enabled: true,
            color: None,
        };
        let action = lang.text("Open", "Открыть").to_owned();
        let run = lang.text("Apply", "Применить").to_owned();
        match self.view.settings.page {
            SettingsPage::General => vec![
                make(lang.text("Language", "Язык"), if lang == crate::model::Language::English { "English" } else { "Русский" }.into(), lang.text("Language of menus, descriptions, notices and keyboard hints.", "Язык меню, описаний, уведомлений и подсказок клавиш."), SettingControl::Language, true),
                make(lang.text("Volume", "Громкость"), format!("{}%", (self.settings.volume*100.0).round() as u32), lang.text("Playback volume. Changes immediately; step: 5%.", "Громкость воспроизведения. Меняется сразу; шаг: 5%."), SettingControl::Volume, true),
                make(lang.text("Sorting desk", "Сортировочный стол"), lang.text(if self.settings.sorting_desk { "On" } else { "Off" }, if self.settings.sorting_desk { "Включён" } else { "Выключен" }).into(), lang.text("Route added files to the editable desk instead of the selected playlist.", "Направлять добавляемые файлы на доступный для правки стол вместо выбранного плейлиста."), SettingControl::Desk, true),
                make(lang.text("Playlists panel", "Панель плейлистов"), self.settings.playlist_placement.name(lang).into(), lang.text("Position of the playlist list relative to the tracks.", "Расположение списка плейлистов относительно композиций."), SettingControl::PlaylistPlacement, true),
                make(lang.text("Player panel", "Панель проигрывателя"), self.settings.player_placement.name(lang).into(), lang.text("Position of playback controls relative to the library.", "Расположение управления воспроизведением относительно библиотеки."), SettingControl::PlayerPlacement, true),
                make(lang.text("Playback timeline", "Дорожка проигрывания"), self.settings.appearance.playback_timeline.name(lang).into(), lang.text("Choose waveform or progress bar. Only the selected view is shown, in both GUI and TUI. Saves into the active profile.", "Выберите аудиоволну или обычную полосу. Показывается только выбранный вид, в GUI и TUI. Выбор сохраняется в активный профиль."), SettingControl::PlaybackTimeline, true),
                make(lang.text("Shortcuts on buttons", "Клавиши на кнопках"), lang.text(if self.settings.appearance.show_button_shortcuts { "Shown" } else { "Hidden" }, if self.settings.appearance.show_button_shortcuts { "Показаны" } else { "Скрыты" }).into(), lang.text("Show assigned shortcuts on buttons. Help always shows F1. Other shortcuts work when hidden, and remain listed in Help and menus. Saved in the active profile.", "Показывать назначенные сочетания на кнопках. На справке всегда остается F1. Остальные клавиши работают при скрытии; их список остается в справке и меню. Выбор сохраняется в активном профиле."), SettingControl::ButtonShortcuts, true),
            ],
            SettingsPage::Profiles => {
                let profile = &self.settings.profiles[self.view.settings.profile.min(self.settings.profiles.len()-1)];
                let mut rows = vec![
                    make(lang.text("Profile", "Профиль"), format!("{}{}", profile.name, if profile.id == self.settings.active_profile { lang.text(" (active)", " (активный)") } else { "" }), lang.text("Browse saved configurations. Applying a profile keeps your music library and current mode.", "Выбор сохранённой конфигурации. Применение сохраняет музыкальную библиотеку и текущий режим."), SettingControl::ProfilePick, true),
                    make(lang.text("Use selected profile", "Использовать профиль"), run.clone(), lang.text("Restore its language, volume, layout, appearance and shortcuts. Later changes save into this profile automatically.", "Восстановить язык, громкость, расположение, оформление и клавиши. Последующие изменения сохраняются в этом профиле автоматически."), SettingControl::ProfileApply, false),
                    make(lang.text("Create profile", "Создать профиль"), action.clone(), lang.text("Name a copy of the current settings and make it active. The source profile is kept.", "Назвать копию текущих настроек и сделать её активной. Исходный профиль сохраняется."), SettingControl::ProfileCreate, false),
                    make(lang.text("Rename profile", "Переименовать профиль"), action.clone(), lang.text("Change the selected profile name without changing its settings.", "Изменить имя выбранного профиля, сохранив его настройки."), SettingControl::ProfileRename, false),
                    make(lang.text("Delete profile", "Удалить профиль"), action.clone(), lang.text("Remove after confirmation. Deleting the active profile switches to another. Keep at least one profile.", "Удалить после подтверждения. При удалении активного профиля включается другой. Один профиль должен остаться."), SettingControl::DeleteCatalog, false),
                ]; rows.last_mut().unwrap().enabled = self.settings.profiles.len() > 1; rows
            }
            SettingsPage::Themes => {
                let preset = &self.settings.presets[self.selected_settings_preset_index()];
                let active_name = self.settings.current_preset().map(|preset| preset.name.as_str()).unwrap_or(lang.text("Custom", "Пользовательская"));
                let palette_name = |mode: Mode| self.settings.palettes.iter().find(|p| p.id == self.settings.appearance.palette_ids[mode.index()]).map(|p| p.name.clone()).unwrap_or_default();
                let mut rows = vec![
                    make(lang.text("Theme preset", "Пресет темы"), active_name.into(), lang.text("Selection applies and saves immediately. Custom means the appearance differs from saved presets. Palette is per mode; font and geometry are shared.", "Выбор сразу применяет и сохраняет пресет. Пользовательская: оформление отличается от пресетов. Палитра для текущего режима, шрифт и геометрия общие."), SettingControl::PresetPick, true),
                    make(lang.text("Order palette", "Палитра Порядка"), palette_name(Mode::Order), lang.text("Colors used while the Order mode is active.", "Цвета интерфейса в режиме Порядка."), SettingControl::ModePalette(Mode::Order), true),
                    make(lang.text("Chaos palette", "Палитра Хаоса"), palette_name(Mode::Chaos), lang.text("Colors used while the Chaos mode is active.", "Цвета интерфейса в режиме Хаоса."), SettingControl::ModePalette(Mode::Chaos), true),
                    make(lang.text("Create preset", "Создать пресет"), action.clone(), lang.text("Save the current mode's palette, font, size and borders under a new name.", "Сохранить палитру текущего режима, шрифт, размер и рамки под новым именем."), SettingControl::PresetCreate, false),
                    make(lang.text("Rename preset", "Переименовать пресет"), preset.name.clone(), lang.text("Change the named preset's name.", "Изменить имя указанного пресета."), SettingControl::PresetRename, false),
                    make(lang.text("Update preset", "Обновить пресет"), preset.name.clone(), lang.text("Replace the named preset's appearance with the current settings.", "Заменить оформление указанного пресета текущими настройками."), SettingControl::PresetUpdate, false),
                    make(lang.text("Delete preset", "Удалить пресет"), preset.name.clone(), lang.text("Remove the named preset after confirmation. The current appearance is kept; one preset must remain.", "Удалить указанный пресет после подтверждения. Текущее оформление сохраняется; один пресет должен остаться."), SettingControl::DeleteCatalog, false),
                ]; rows.last_mut().unwrap().enabled = self.settings.presets.len() > 1; rows
            }
            SettingsPage::Palettes => {
                let palette = self.selected_settings_palette();
                let mut rows = vec![
                    make(lang.text("Palette", "Палитра"), format!("{} ({}/{})", palette.name, self.view.settings.palette.min(self.settings.palettes.len()-1)+1, self.settings.palettes.len()), lang.text("Browse all palettes. Color edits update every theme and profile using this palette.", "Выбор палитры. Изменения цветов отражаются во всех темах и профилях, использующих эту палитру."), SettingControl::PalettePick, true),
                    make(lang.text("Use in current mode", "Использовать в текущем режиме"), run.clone(), lang.text("Set this palette for the current mode; keep font and geometry.", "Включить палитру в текущем режиме, сохранив шрифт и геометрию."), SettingControl::PaletteApply, false),
                    make(lang.text("Rename palette", "Переименовать палитру"), action.clone(), lang.text("Change the name without breaking theme or profile references.", "Изменить имя, сохранив связи с темами и профилями."), SettingControl::PaletteRename, false),
                ];
                for role in color_roles() {
                    let color = palette.color(&role.token);
                    let label = color_role_name(&role.token, lang);
                    let description = if lang == crate::model::Language::English { &role.role_en } else { &role.role_ru };
                    let mut row = make(label, color_hex(color), description, SettingControl::PaletteColor(role.token.clone()), false);
                    row.color = Some(color); rows.push(row);
                }
                rows.extend([
                    make(lang.text("Create palette", "Создать палитру"), action.clone(), lang.text("Create an independently editable copy of the selected palette.", "Создать независимо редактируемую копию выбранной палитры."), SettingControl::PaletteCreate, false),
                    make(lang.text("Import palette", "Импортировать палитру"), action.clone(), lang.text("Read an Almavorn or Molten-Zharr JSON file. Import adds a copy; existing colors remain intact.", "Прочитать JSON Almavorn или Molten-Zharr. Импорт добавляет копию и сохраняет существующие цвета."), SettingControl::PaletteImport, false),
                    make(lang.text("Export palette", "Экспортировать палитру"), action.clone(), lang.text("Write all color roles into a new JSON file. Existing files are not overwritten.", "Сохранить все цветовые роли в новый JSON. Существующие файлы не перезаписываются."), SettingControl::PaletteExport, false),
                    make(lang.text("Delete palette", "Удалить палитру"), action, lang.text("Remove after confirmation. Themes and profiles using it switch to the first remaining palette.", "Удалить после подтверждения. Использующие её темы и профили переходят на первую оставшуюся палитру."), SettingControl::DeleteCatalog, false),
                ]);
                rows.last_mut().unwrap().enabled = self.settings.palettes.len() > 1;
                for row in &mut rows {
                    if matches!(row.control, SettingControl::PaletteImport | SettingControl::PaletteExport) { row.enabled = !self.settings_file_busy(); }
                }
                rows
            }
            SettingsPage::Typography => {
                let appearance = &self.settings.appearance;
                let mut rows = vec![
                    make(lang.text("Font", "Шрифт"), appearance.font.name().into(), lang.text("Desktop font with Cyrillic support. In TUI, the terminal controls the font.", "Шрифт окна с поддержкой кириллицы. В TUI шрифтом управляет терминал."), SettingControl::Font, true),
                    make(lang.text("Font size", "Размер шрифта"), format!("{} px", appearance.font_size), lang.text("Desktop text size, 10-32 pixels. Arrows change by 1; Enter accepts an exact value. TUI uses terminal size.", "Размер текста в окне: 10-32 пикселя. Стрелки меняют на 1; Enter - точное значение. В TUI размер задаёт терминал."), SettingControl::FontSize, true),
                    make(lang.text("Borders", "Рамки"), appearance.borders.name(lang).into(), lang.text("Panel and dialog borders: none, single, double or thick. Applies to GUI and TUI.", "Рамки панелей и диалогов: без рамки, одинарные, двойные или толстые. Работает в GUI и TUI."), SettingControl::Borders, true),
                    make(lang.text("Corners", "Углы"), appearance.corners.name(lang).into(), lang.text("Square or rounded corners for single borders. Double and thick borders have square corners.", "Прямые или скруглённые углы одинарных рамок. Двойные и толстые рамки имеют прямые углы."), SettingControl::Corners, true),
                ]; rows.last_mut().unwrap().enabled = appearance.borders == BorderWeight::Single; rows
            }
            SettingsPage::Shortcuts => {
                let mut rows = vec![make(lang.text("Reset shortcuts", "Сбросить горячие клавиши"), run, lang.text("Restore standard shortcuts in this profile. Settings navigation remains available.", "Восстановить стандартные сочетания в этом профиле. Навигация настроек остаётся доступной."), SettingControl::ResetBindings, false)];
                rows.extend(self.settings.bindings.iter().enumerate().map(|(index, binding)| make(binding.action.name(lang), binding.key.label(), binding.action.description(lang), SettingControl::Binding(index), false)));
                rows
            }
        }
    }
}
