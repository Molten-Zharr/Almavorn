use super::{App, SettingsPage};
use crate::{
    input::ShortcutGroup,
    model::{Language, Mode},
    preferences::{BorderWeight, color_hex, color_role_name, color_roles},
};

#[derive(Clone, Debug)]
pub(crate) enum SettingControl {
    LyricsEnabled,
    LyricsPanel,
    LyricsProvider(crate::lyrics::Provider),
    Language,
    Volume,
    Desk,
    PlaylistPlacement,
    PlayerPlacement,
    PlaybackTimeline,
    ButtonShortcuts,
    PlayerButtonSize,
    PlayerSpecialButtons,
    QuickAccess,
    ArtworkVisible,
    ArtworkSize,
    ArtworkShared,
    ArtworkPath,
    Visualizer,
    AnimationBrightness,
    GifFile,
    GifOpacity,
    GifClear,
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
    SystemMedia,
    NiriBindings,
    DeleteCatalog,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingGroup {
    Shortcuts(ShortcutGroup),
    DangerZone,
}
impl SettingGroup {
    pub(crate) fn name(self, language: Language) -> &'static str {
        match self {
            Self::Shortcuts(group) => group.name(language),
            Self::DangerZone => language.text("Danger zone", "Опасная зона"),
        }
    }
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
    pub group: Option<SettingGroup>,
}
impl App {
    pub(crate) fn settings_rows(&self) -> Vec<SettingRow> {
        let lang = self.settings.preferences.language;
        let make = |label: &str,
                    value: String,
                    description: &str,
                    control: SettingControl,
                    adjustable: bool| {
            let group = match &control {
                SettingControl::Binding(index) => Some(SettingGroup::Shortcuts(
                    self.settings.preferences.bindings[*index]
                        .action
                        .shortcut_group(),
                )),
                SettingControl::PaletteImport
                | SettingControl::PaletteExport
                | SettingControl::DeleteCatalog => Some(SettingGroup::DangerZone),
                _ => None,
            };
            SettingRow {
                label: label.into(),
                value,
                description: description.into(),
                control,
                adjustable,
                enabled: true,
                color: None,
                group,
            }
        };
        let action = lang.text("Open", "Открыть").to_owned();
        let run = lang.text("Apply", "Применить").to_owned();
        match self.view.settings.page {
            SettingsPage::Mods | SettingsPage::ModGuide | SettingsPage::Services
            | SettingsPage::ServiceTracks | SettingsPage::ServicePlaylists
            | SettingsPage::ServiceDevices | SettingsPage::ServiceGuide => Vec::new(),
            SettingsPage::Lyrics => {
                let preferences = &self.settings.preferences.lyrics;
                let checked = |value| if value { "[x]".to_owned() } else { "[ ]".to_owned() };
                let mut rows = vec![
                    make(lang.text("Enable lyrics search", "Включить поиск текстов песен"), checked(preferences.enabled), lang.text("Allow online searches. When disabled, only saved lyrics and file tags are read.", "Разрешить поиск в интернете. При отключении читаются только сохраненные тексты и теги файлов."), SettingControl::LyricsEnabled, true),
                    make(lang.text("Show lyrics", "Показывать текст"), checked(preferences.show_panel), lang.text("Show a separate lyrics block and search when tracks change. Move and resize it in Layout mode.", "Показывать отдельный блок текста и искать его при смене композиции. Положение и размер меняются в режиме Блоки."), SettingControl::LyricsPanel, true),
                ];
                rows.extend(crate::lyrics::Provider::ALL.into_iter().map(|provider| make(provider.name(), checked(!preferences.disabled.contains(&provider)), lang.text("Include this source in searches. Genius has priority among confident matches.", "Искать в этом источнике. При уверенном совпадении первым выбирается Genius."), SettingControl::LyricsProvider(provider), true)));
                rows
            }
            SettingsPage::General => vec![
                make(lang.text("Quick access", "Панель быстрого доступа"), lang.text("Customize…", "Настроить…").into(), lang.text("Choose the buttons above the library. Unpinned commands stay in More. You can show all buttons, hide them all or display active modes automatically.", "Выберите кнопки над библиотекой. Остальные команды останутся в Больше. Можно показать все кнопки, убрать все или автоматически показывать включенные режимы."), SettingControl::QuickAccess, false),
                make(lang.text("Language", "Язык"), if lang == crate::model::Language::English { "English" } else { "Русский" }.into(), lang.text("Language of menus, descriptions, notices and keyboard hints.", "Язык меню, описаний, уведомлений и подсказок клавиш."), SettingControl::Language, true),
                make(lang.text("Volume", "Громкость"), format!("{}%", (self.settings.preferences.volume*100.0).round() as u32), lang.text("Playback volume. Changes immediately; step: 5%.", "Громкость воспроизведения. Меняется сразу; шаг: 5%."), SettingControl::Volume, true),
                make(lang.text("Sorting desk", "Сортировочный стол"), lang.text(if self.settings.preferences.sorting_desk { "On" } else { "Off" }, if self.settings.preferences.sorting_desk { "Включён" } else { "Выключен" }).into(), lang.text("In Chaos, route added music to the sorting desk instead of the selected playlist. In Order, music always goes to the desk, even with editing off.", "В Хаосе направлять добавляемую музыку в сортировочный стол вместо выбранного плейлиста. В Порядке музыка всегда добавляется в стол, даже без включения правки."), SettingControl::Desk, true),
                make(lang.text("Playlists panel", "Панель плейлистов"), self.settings.preferences.playlist_placement.name(lang).into(), lang.text("Position of the playlist list relative to the tracks.", "Расположение списка плейлистов относительно композиций."), SettingControl::PlaylistPlacement, true),
                make(lang.text("Player panel", "Панель проигрывателя"), self.settings.preferences.player_placement.name(lang).into(), lang.text("Position of playback controls relative to the library.", "Расположение управления воспроизведением относительно библиотеки."), SettingControl::PlayerPlacement, true),
                make(lang.text("Playback timeline", "Дорожка проигрывания"), self.settings.preferences.appearance.playback_timeline.name(lang).into(), lang.text("Choose waveform or progress bar. Only the selected view is shown, in both GUI and TUI. Saves into the active profile.", "Выберите аудиоволну или обычную полосу. Показывается только выбранный вид, в GUI и TUI. Выбор сохраняется в активный профиль."), SettingControl::PlaybackTimeline, true),
                make(lang.text("Shortcuts on buttons", "Клавиши на кнопках"), lang.text(if self.settings.preferences.appearance.show_button_shortcuts { "Shown" } else { "Hidden" }, if self.settings.preferences.appearance.show_button_shortcuts { "Показаны" } else { "Скрыты" }).into(), lang.text("Show shortcuts on buttons and compact toolbar menus. Change bindings under Shortcuts. Hiding hints keeps shortcuts active.", "Показывать сочетания на кнопках и меню свернутой верхней панели. Назначения меняются в разделе Клавиши; скрытие подсказок не отключает сочетания."), SettingControl::ButtonShortcuts, true),
                make(lang.text("Player button size", "Размер кнопок проигрывателя"), self.settings.preferences.appearance.player_button_size.name(lang).into(), lang.text("Choose compact, normal or large square playback buttons. The symbols grow with the buttons. Shortcut labels appear below them.", "Компактные, обычные или крупные квадратные кнопки воспроизведения. Значки увеличиваются вместе с кнопками. Подписи горячих клавиш показаны снизу."), SettingControl::PlayerButtonSize, true),
                make(lang.text("Show player special keys", "Показывать спецклавиши проигрывателя"), if self.settings.preferences.appearance.show_player_special_buttons { "[x]" } else { "[ ]" }.into(), lang.text("Show equalizer, effects and visualization buttons beside the playback controls. Their commands remain available in the player menu.", "Показывать кнопки эквалайзера, эффектов и визуализации рядом с кнопками воспроизведения. Их команды остаются в меню проигрывателя."), SettingControl::PlayerSpecialButtons, true),
                make(lang.text("Player artwork", "Обложка в проигрывателе"), if self.settings.preferences.artwork.visible { "[x]" } else { "[ ]" }.into(), lang.text("Show an image next to the playing track. When there is no artwork, show a music note.", "Показывать изображение рядом с текущей композицией. Если обложки нет, показывать музыкальную ноту."), SettingControl::ArtworkVisible, true),
                make(lang.text("Track thumbnail size", "Размер миниатюры трека"), self.settings.preferences.artwork.size.name(lang).into(), lang.text("Size of the artwork in the player.", "Размер обложки в проигрывателе."), SettingControl::ArtworkSize, true),
                make(lang.text("One image for all tracks", "Общая картинка"), if self.settings.preferences.artwork.shared { "[x]" } else { "[ ]" }.into(), lang.text("Use the image at the path below for every track. When unchecked or no path is set, use the artwork embedded in the music file.", "Использовать картинку по указанному ниже пути для всех композиций. Если галочка снята или путь не указан, брать обложку из музыкального файла."), SettingControl::ArtworkShared, true),
                make(lang.text("Shared image path", "Путь к общей картинке"), self.settings.preferences.artwork.path.as_ref().map_or_else(|| lang.text("Not selected", "Не выбрана").into(), |path| path.display().to_string()), lang.text("Full path to a PNG or JPEG image. Clear the field to remove the path.", "Полный путь к картинке PNG или JPEG. Очистите поле, чтобы убрать путь."), SettingControl::ArtworkPath, false),
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
                let palette_name = |mode: Mode| self.settings.palettes.iter().find(|p| p.id == self.settings.preferences.appearance.palette_ids[mode.index()]).map(|p| p.name.clone()).unwrap_or_default();
                let mut rows = vec![
                    make(lang.text("Theme preset", "Пресет темы"), active_name.into(), lang.text("Selection applies and saves immediately. Custom means the appearance differs from saved presets. Palette is per mode; font, geometry and animation are shared.", "Выбор сразу применяет и сохраняет пресет. Пользовательская: оформление отличается от пресетов. Палитра для текущего режима, шрифт, геометрия и анимация общие."), SettingControl::PresetPick, true),
                    make(lang.text("Order palette", "Палитра Порядка"), palette_name(Mode::Order), lang.text("Colors used while the Order mode is active.", "Цвета интерфейса в режиме Порядка."), SettingControl::ModePalette(Mode::Order), true),
                    make(lang.text("Chaos palette", "Палитра Хаоса"), palette_name(Mode::Chaos), lang.text("Colors used while the Chaos mode is active.", "Цвета интерфейса в режиме Хаоса."), SettingControl::ModePalette(Mode::Chaos), true),
                    make(lang.text("Create preset", "Создать пресет"), action.clone(), lang.text("Save the current palette, font, borders, background scene and brightness under a new name.", "Сохранить палитру, шрифт, рамки, фоновую сцену и ее яркость под новым именем."), SettingControl::PresetCreate, false),
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
            SettingsPage::Animation => {
                let mut rows = vec![
                make(lang.text("Background animation", "Анимация фона"), self.settings.preferences.appearance.visualizer.name(lang).into(), lang.text("Choose a built-in scene, a GIF file, or No animation. Built-in scenes follow the music and pause with it. GIF playback loops independently.", "Выберите встроенную сцену, GIF из файла или пункт Без анимации. Встроенные сцены реагируют на музыку и замирают на паузе. GIF повторяется независимо от музыки."), SettingControl::Visualizer, true),
                make(lang.text("Built-in animation brightness", "Яркость встроенных анимаций"), format!("{}%", self.settings.preferences.appearance.animation_brightness.clamp(10, 70)), lang.text("Change the brightness from 10 to 70%, in steps of 5%. Save or update a theme preset to keep its scene and brightness together.", "Яркость от 10 до 70%, шаг 5%. Создайте или обновите пресет темы, чтобы сохранить в нем сцену и яркость."), SettingControl::AnimationBrightness, true),
                make(lang.text("Background GIF", "GIF-фон"), self.background_gif_status(), lang.text("Choose or replace a local GIF. The file picker supports filtering and sorting. GIF playback loops independently of music. Save or update a theme preset to keep the file and its visibility.", "Выберите или замените GIF на устройстве. В окне выбора есть фильтр и сортировка. GIF повторяется независимо от музыки. Создайте или обновите пресет темы, чтобы сохранить файл и видимость фона."), SettingControl::GifFile, false),
                make(lang.text("Background visibility", "Видимость фона"), format!("{}%", self.settings.preferences.appearance.gif_background.opacity.min(30)), lang.text("GIF visibility from 0 to 30%; default 15%. Text and controls remain opaque. Drag the slider or use the arrows.", "Видимость GIF от 0 до 30%, по умолчанию 15%. Текст и кнопки остаются непрозрачными. Перетащите ползунок или используйте стрелки."), SettingControl::GifOpacity, true),
                make(lang.text("Remove background GIF", "Убрать GIF-фон"), action, lang.text("Remove the GIF from these settings. The original file stays on disk.", "Убрать GIF из этих настроек. Исходный файл останется на диске."), SettingControl::GifClear, false),
                ];
                rows[1].enabled = self.settings.preferences.appearance.visualizer != crate::visualizer::VisualizerKind::Gif;
                rows[4].enabled = self.settings.preferences.appearance.gif_background.path.is_some();
                rows
            },
            SettingsPage::Typography => {
                let appearance = &self.settings.preferences.appearance;
                let mut rows = vec![
                    make(lang.text("Font", "Шрифт"), appearance.font.name().into(), lang.text("Desktop font with Cyrillic support. In TUI, the terminal controls the font.", "Шрифт окна с поддержкой кириллицы. В TUI шрифтом управляет терминал."), SettingControl::Font, true),
                    make(lang.text("Font size", "Размер шрифта"), format!("{} px", appearance.font_size), lang.text("Desktop text size, 10-32 pixels. Arrows change by 1; Enter accepts an exact value. TUI uses terminal size.", "Размер текста в окне: 10-32 пикселя. Стрелки меняют на 1; Enter - точное значение. В TUI размер задаёт терминал."), SettingControl::FontSize, true),
                    make(lang.text("Borders", "Рамки"), appearance.borders.name(lang).into(), lang.text("Panel and dialog borders: none, single, double or thick. Applies to GUI and TUI.", "Рамки панелей и диалогов: без рамки, одинарные, двойные или толстые. Работает в GUI и TUI."), SettingControl::Borders, true),
                    make(lang.text("Corners", "Углы"), appearance.corners.name(lang).into(), lang.text("Square or rounded corners for single borders. Double and thick borders have square corners.", "Прямые или скруглённые углы одинарных рамок. Двойные и толстые рамки имеют прямые углы."), SettingControl::Corners, true),
                ]; rows.last_mut().unwrap().enabled = appearance.borders == BorderWeight::Single; rows
            }
            SettingsPage::Shortcuts => {
                let mut rows = vec![make(lang.text("Reset shortcuts", "Сбросить горячие клавиши"), run, lang.text("Restore standard shortcuts in this profile. Settings navigation remains available.", "Восстановить стандартные сочетания в этом профиле. Навигация настроек остаётся доступной."), SettingControl::ResetBindings, false)];
                rows.push(make(lang.text("Media controls · MPRIS", "Управление · MPRIS"), self.system_media_status(), lang.text("Playback controls over the session D-Bus: media keys, desktop panels and playerctl. Enabled automatically. Custom global keys are assigned in your desktop settings.", "Управление через D-Bus: медиаклавиши, системные панели и playerctl. Подключается автоматически. Произвольные глобальные сочетания назначаются в настройках окружения."), SettingControl::SystemMedia, false));
                rows.push(make(lang.text("Additional setup · niri", "Дополнительно · niri"), lang.text("Configuration", "Конфиг").into(), lang.text("Optional manual export for niri. Set keys in each shortcut editor, then add this block to config.kdl. Other desktops can assign Almavorn commands or use MPRIS.", "Дополнительный ручной экспорт для niri. Задайте клавиши в окне действия, затем добавьте блок в config.kdl. В других окружениях назначайте команды Almavorn или используйте MPRIS."), SettingControl::NiriBindings, false));
                let mut bindings: Vec<_> = self.settings.preferences.bindings.iter().enumerate().collect();
                bindings.sort_by_key(|(_, binding)| binding.action.shortcut_group());
                rows.extend(bindings.into_iter().map(|(index, binding)| {
                    let global = self.settings.preferences.global_shortcuts.iter().any(|global| global.action == binding.action && global.enabled);
                    let value = if global { format!("{} · niri", binding.key.label()) } else { binding.key.label() };
                    make(binding.action.name(lang), value, binding.action.description(lang), SettingControl::Binding(index), false)
                }));
                rows
            }
        }
    }
}
