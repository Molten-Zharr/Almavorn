use crate::{
    app::App,
    input::{Action, Key, KeyPress},
    model::Language,
};

#[derive(Clone, Copy)]
enum Category {
    Navigation,
    Playback,
    Playlists,
    Tracks,
    Adding,
    Search,
    Layout,
    Settings,
    Palette,
    Equalizer,
    Effects,
    Tags,
    Lyrics,
    Downloads,
    Services,
    Mods,
    Statistics,
    Composition,
    Text,
    Mouse,
    System,
    Guide,
}

const CATEGORIES: &[(Category, &str, &str)] = &[
    (
        Category::Navigation,
        "Navigation and menus",
        "Навигация и меню",
    ),
    (Category::Playback, "Player", "Проигрыватель"),
    (Category::Playlists, "Playlists", "Плейлисты"),
    (Category::Tracks, "Tracks", "Композиции"),
    (Category::Adding, "Adding music", "Добавление музыки"),
    (Category::Search, "Search and filter", "Поиск и фильтр"),
    (Category::Layout, "Blocks and layout", "Блоки и раскладка"),
    (
        Category::Settings,
        "Settings and keys",
        "Настройки и клавиши",
    ),
    (Category::Palette, "Color picker", "Выбор цвета"),
    (Category::Equalizer, "Equalizer", "Эквалайзер"),
    (Category::Effects, "Playback effects", "Эффекты"),
    (Category::Tags, "Tags and file names", "Теги и имена файлов"),
    (Category::Lyrics, "Lyrics", "Тексты песен"),
    (Category::Downloads, "Downloads", "Загрузки"),
    (Category::Services, "Music services", "Музыкальные сервисы"),
    (Category::Mods, "Mods", "Моды"),
    (Category::Statistics, "Statistics", "Статистика"),
    (
        Category::Composition,
        "Building playlists",
        "Сборка плейлистов",
    ),
    (
        Category::Text,
        "Text and confirmation",
        "Ввод и подтверждения",
    ),
    (Category::Mouse, "Mouse and scrolling", "Мышь и прокрутка"),
    (Category::System, "Global shortcuts", "Глобальные клавиши"),
    (Category::Guide, "Getting started", "Как пользоваться"),
];

pub(super) struct Section {
    pub name: &'static str,
    pub search_name: String,
    pub entries: Vec<Entry>,
    language: Language,
}

pub(super) struct Entry {
    pub key: Option<KeyPress>,
    pub text: String,
    pub search: String,
    pub aliases: &'static str,
}

impl Entry {
    fn new(key: Option<KeyPress>, text: String, search: String) -> Self {
        Self { key, text, search, aliases: "" }
    }
    fn heading(text: String) -> Self { Self::new(None, text, String::new()) }
}

impl Section {
    fn keys(&mut self, keys: &[KeyPress], en: &'static str, ru: &'static str) {
        for &key in keys {
            self.entries
                .push(Entry::new(Some(key), self.language.text(en, ru).into(), format!("{en} {ru}")));
        }
    }
    fn note(&mut self, en: &'static str, ru: &'static str) {
        self.entries.push(Entry::new(None, self.language.text(en, ru).into(), format!("{en} {ru}")));
    }
    fn topic(&mut self, en: &'static str, ru: &'static str, aliases: &'static str) {
        self.note(en, ru);
        self.entries.last_mut().expect("topic entry").aliases = aliases;
    }
}

fn p(key: Key) -> KeyPress {
    KeyPress::plain(key)
}
fn c(key: Key) -> KeyPress {
    KeyPress {
        ctrl: true,
        ..p(key)
    }
}
fn s(key: Key) -> KeyPress {
    KeyPress {
        shift: true,
        ..p(key)
    }
}
fn a(key: Key) -> KeyPress {
    KeyPress {
        alt: true,
        ..p(key)
    }
}
fn cs(key: Key) -> KeyPress {
    KeyPress {
        shift: true,
        ..c(key)
    }
}
fn ca(key: Key) -> KeyPress {
    KeyPress {
        alt: true,
        ..c(key)
    }
}

// Exhaustive so a new configurable action cannot silently disappear from help.
fn categories(action: Action) -> &'static [Category] {
    use {Action::*, Category as C};
    match action {
        OpenAddMenu | OpenPlaylistMenu | OpenTracksMenu | OpenApplicationMenu | ToggleToolbar
        | Quit | Help | SwitchMode => &[C::Navigation],
        Settings => &[C::Settings],
        Statistics => &[C::Statistics],
        TogglePlay | Stop | Next | Previous | CycleRepeat | ToggleShuffle | VolumeUp
        | VolumeDown | SeekForward | SeekBackward | CycleVisualizer => &[C::Playback],
        Equalizer => &[C::Playback, C::Equalizer],
        PlaybackTuner => &[C::Playback, C::Effects],
        NewPlaylist | CopyPlaylist | PlaylistFolders | PlaylistUp | PlaylistDown | ToggleEdit => {
            &[C::Playlists]
        }
        Rename | Delete | MoveUp | MoveDown | Undo | Redo => &[C::Playlists, C::Tracks],
        Mark | PlayMarked | PlayWholePlaylist | Transfer | Sort | Metadata => &[C::Tracks],
        EditTags => &[C::Tracks, C::Tags],
        ComposePlaylists => &[C::Playlists, C::Composition],
        ToggleDesk | AddFiles | AddFolder | AddFolderPlaylists => &[C::Adding],
        AddNew => &[C::Adding, C::Tracks],
        Search | Filter => &[C::Search],
        PlaylistPanel | PlayerPanel | Panels => &[C::Layout],
        Lyrics => &[C::Lyrics],
        Downloads => &[C::Downloads],
        Services => &[C::Services],
        Mods => &[C::Mods],
    }
}

pub(super) fn sections(app: &App, guide: Vec<String>) -> Vec<Section> {
    use {Category as C, Key::*};
    let language = app.settings.preferences.language;
    let mut sections: Vec<_> = CATEGORIES
        .iter()
        .map(|(_, en, ru)| Section {
            name: language.text(en, ru),
            search_name: format!("{en} {ru}"),
            entries: Vec::new(),
            language,
        })
        .collect();
    for binding in &app.settings.preferences.bindings {
        for category in categories(binding.action) {
            sections[*category as usize].entries.push(Entry {
                key: Some(binding.key),
                text: binding.action.description(language).into(),
                search: format!("{} {} {} {}", binding.action.name(Language::English),
                    binding.action.name(Language::Russian), binding.action.description(Language::English),
                    binding.action.description(Language::Russian)),
                aliases: crate::help_search::aliases(binding.action),
            });
        }
    }
    let configured: Vec<_> = sections
        .iter()
        .map(|section| section.entries.len())
        .collect();

    let q = &mut sections[C::Navigation as usize];
    q.note("In Help, type a question or a shortcut. Search understands synonyms and common typos. Up/Down chooses a result; Enter opens its section. Esc clears the search first, then closes Help.",
        "В справке введите вопрос или сочетание клавиш. Поиск учитывает синонимы и небольшие опечатки. Стрелки вверх и вниз выбирают результат, Enter открывает его раздел. Esc сначала очищает поиск, затем закрывает справку.");
    q.keys(&[c(Char('f'))], "In Help, select the search query to replace it.",
        "В справке выделить поисковый запрос для замены.");
    q.note("These shortcuts follow your settings. Letter shortcuts also work with a Russian keyboard layout. Each window's keys are listed in its own section.",
        "Здесь показаны клавиши из ваших настроек. Буквенные команды работают и в русской раскладке. Клавиши каждого окна — в его разделе справки.");
    q.keys(&[p(Escape)], "Close the open window, even after clicking the player. In an editor, go back. In a file picker, clear the filter first.", "Закрыть открытое окно, даже после клика по плееру. В редакторе — вернуться назад. В выборе файлов — сначала очистить фильтр.");
    q.keys(
        &[p(F(10))],
        "Open the active panel's menu. Right-click opens the same menu.",
        "Открыть меню активной панели. Правый клик открывает то же меню.",
    );
    q.keys(
        &[p(Tab)],
        "Go to the next panel or the top menu. If a top menu is open, leave it.",
        "Перейти к следующей панели или верхнему меню. Если верхнее меню открыто — выйти из него.",
    );
    q.keys(
        &[s(Tab)],
        "Move to the previous panel or the command bar.",
        "Перейти к предыдущей панели или строке команд.",
    );
    q.keys(
        &[p(Up), p(Down)],
        "Select a row or a top-menu button. In the player, change volume by 1%.",
        "Выбрать строку или кнопку верхнего меню. В проигрывателе — изменить громкость на 1%.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "Switch top menus. In Help, switch sections.",
        "Переключить верхнее меню. В справке — перейти к соседнему разделу.",
    );
    q.keys(
        &[p(Enter)],
        "Open a playlist, play the selected track or run a menu command.",
        "Открыть плейлист, включить выбранный трек или выполнить команду меню.",
    );
    q.keys(
        &[p(Char(' '))],
        "On the command bar, open the selected button's menu.",
        "В строке команд открыть меню выбранной кнопки.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move by 10 rows in a playlist. In Help, scroll one page.",
        "Перейти на 10 строк в плейлисте. В справке — прокрутить на страницу.",
    );
    q.keys(
        &[p(Home), p(End)],
        "Go to the beginning or end of the library list, command bar or Help.",
        "Перейти в начало или конец списка библиотеки, строки команд или помощи.",
    );
    q.keys(&[p(Tab), s(Tab)], "In Help: next or previous category. The category list also supports clicks and the mouse wheel.", "В помощи: следующая или предыдущая категория. Список категорий также поддерживает клик и колесо мыши.");

    let q = &mut sections[C::Playlists as usize];
    q.note("In Order, click Edit before moving a playlist. Chaos allows changes right away. The sorting desk stays first.",
        "Чтобы переместить плейлист в Порядке, сначала нажмите \"Редактировать\". В Хаосе можно менять сразу. Сортировочный стол всегда стоит первым.");
    q.note(
        "The playlist up/down shortcuts also work while you are selecting tracks.",
        "Клавиши перемещения плейлиста работают и тогда, когда вы выбираете треки.",
    );
    q.keys(
        &[c(Char('d'))],
        "When creating a playlist: choose whether to create a folder on disk.",
        "При создании плейлиста: включить или выключить создание папки на диске.",
    );
    q.keys(&[c(Char('f'))], "When creating a playlist: choose folders with music. If creating a new folder, choose where to put it.", "При создании плейлиста: выбрать папки с музыкой. Если создаете новую папку — выбрать место для нее.");
    q.keys(
        &[c(Char('g'))],
        "After choosing a source folder: include subfolders as track groups.",
        "После выбора исходной папки: включить ее подпапки как группы композиций.",
    );
    q.note("In Playlist folders:", "В окне \"Папки плейлиста\":");
    q.keys(
        &[p(Insert)],
        "Link another folder. Enable editing first in Order.",
        "Добавить папку для поиска новых треков. В Порядке сначала включите редактирование.",
    );
    q.keys(&[p(F(3)), p(Enter)], "Change the selected folder path. On the desk, these paths follow the tracks and cannot be edited here.", "Изменить путь к выбранной папке. На столе путь определяется самими треками и здесь не меняется.");
    q.keys(
        &[p(Delete)],
        "Unlink the folder. Its tracks stay in the playlist.",
        "Отключить папку. Ее треки останутся в плейлисте.",
    );
    q.keys(
        &[p(Char('r'))],
        "Find new tracks in these folders. On the desk, scan only its group folders.",
        "Найти новые треки в этих папках. На столе — только в папках его групп.",
    );
    q.keys(
        &[p(Char(' '))],
        "In a regular playlist's folder window: toggle searching subfolders.",
        "В окне папок обычного плейлиста: включить или выключить поиск в подпапках.",
    );

    sections[C::Tracks as usize].note("Use checkboxes to select tracks for copying, moving, looping or editing tags together. A group checkbox selects its tracks matching the filter, even when collapsed.",
        "Отметьте треки галочками для копирования, переноса или правки тегов. Галочка группы отмечает подходящие под фильтр треки, даже если группа свернута.");
    sections[C::Tracks as usize].keys(&[p(Enter)],
        "In Tracks to playlist: unlock the selected playlist, then copy the tracks.",
        "В окне выбора плейлиста: открыть выбранный для правки, затем скопировать треки.");
    sections[C::Tracks as usize].keys(&[c(Enter)],
        "In Tracks to playlist, from the desk: choose a folder to move the selected files.",
        "В окне выбора плейлиста, со стола: выбрать папку для переноса отмеченных файлов.");
    sections[C::Tracks as usize].keys(&[c(Char('f'))],
        "In the file move window: enter another destination folder.",
        "В окне переноса файлов: указать другую папку назначения.");
    sections[C::Tracks as usize].note("In Order, click Edit before changing a playlist. The sorting desk is always editable.",
        "Для изменения плейлиста в Порядке нажмите \"Редактировать\". Сортировочный стол можно править всегда.");
    sections[C::Tracks as usize].note(
        "To see your saved track order, sort by Position.",
        "Чтобы видеть свой порядок треков, выберите сортировку \"Позиция\".",
    );

    let q = &mut sections[C::Adding as usize];
    q.note("Add new scans the open playlist. On the desk, it scans the folders of its groups. Run it from the Add music menu to choose which folders to scan.",
        "\"Добавить новые\" проверяет открытый плейлист, а на столе — папки его групп. Чтобы выбрать папки для проверки, вызовите команду из меню добавления музыки.");
    q.note("Folders to playlists creates a playlist for each checked folder. You can also select several folders when creating a playlist from folders.",
        "\"Папки в плейлисты\" создает плейлист из каждой отмеченной папки. Выбрать несколько папок можно и при создании плейлиста из папок.");
    q.note(
        "In file and folder pickers:",
        "В окне выбора файлов и папок:",
    );
    q.keys(
        &[p(Enter)],
        "Open the highlighted folder or toggle a file's mark.",
        "Открыть выделенную папку или переключить отметку файла.",
    );
    q.keys(&[p(Insert), c(Char(' '))], "Check or uncheck a file or folder. In Add folder, Ctrl+Space toggles subfolders.", "Поставить или снять галочку у файла или папки. В окне \"Добавить папку\" Ctrl+Space включает подпапки.");
    q.keys(&[p(Char(' '))], "Check an item if the filter is empty. In Add folder, toggle subfolders. When typing a filter, enter a space.", "Поставить галочку, если фильтр пуст. В окне \"Добавить папку\" — включить подпапки. При вводе фильтра — ввести пробел.");
    q.keys(
        &[c(Char('a'))],
        "Check all files or folders shown in this list. Keep earlier selections.",
        "Отметить все файлы или папки в текущем списке. Прежние отметки сохраняются.",
    );
    q.keys(
        &[c(Char('d'))],
        "Clear all file or folder marks, including those in other folders.",
        "Снять все отметки файлов или папок, включая отметки в других каталогах.",
    );
    q.keys(&[c(Enter)], "Add the checked files or create playlists from checked folders. With no checks, select the highlighted folder, or the open folder after going up.", "Добавить отмеченные файлы или создать плейлисты из отмеченных папок. Если отметок нет — выбрать выделенную папку, а после перехода наверх — открытую.");
    q.keys(
        &[cs(Enter)],
        "Choose the folder you are currently viewing, even if a subfolder is highlighted.",
        "Выбрать папку, которая сейчас открыта, даже если выделена подпапка.",
    );
    q.keys(
        &[a(Up)],
        "Open the parent folder.",
        "Перейти в папку уровнем выше.",
    );
    q.keys(
        &[p(Backspace)],
        "Remove a filter character; when the filter is empty, open the parent folder.",
        "Стереть символ фильтра. Если фильтр пуст — перейти в папку уровнем выше.",
    );
    q.keys(
        &[p(Escape)],
        "Clear a nonempty filter; press again to close the picker.",
        "Очистить непустой фильтр; повторное нажатие закрывает окно.",
    );
    q.keys(
        &[c(Char('s'))],
        "Cycle sorting: name, modification time, size, type.",
        "Сортировать по имени, дате изменения, размеру или типу.",
    );
    q.keys(
        &[cs(Char('s'))],
        "Reverse the current sort order.",
        "Изменить направление сортировки.",
    );
    q.keys(
        &[c(Char('g'))],
        "In Folders to playlists: include subfolders as groups in each new playlist.",
        "В Папки в плейлисты: включить подпапки как группы в каждом новом плейлисте.",
    );
    q.keys(
        &[p(Up), p(Down), p(Home), p(End)],
        "Move through the file list or jump to its first or last row.",
        "Перейти по списку файлов либо к его первой или последней строке.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "In the file picker, move by eight rows. Ctrl+Shift+wheel scrolls a visible page.",
        "В файловом окне перейти на восемь строк. Ctrl+Shift+колесо листает видимыми страницами.",
    );
    q.note("In Find new tracks:", "В окне \"Добавить новые\":");
    q.keys(
        &[p(Char(' ')), p(Insert)],
        "Check or uncheck a folder.",
        "Поставить или снять галочку у папки.",
    );
    q.keys(&[c(Char('a'))], "Check all folders.", "Отметить все папки.");
    q.keys(
        &[c(Char('d'))],
        "Uncheck all folders.",
        "Снять все отметки.",
    );
    q.keys(
        &[p(Enter)],
        "Find new tracks in the checked folders.",
        "Найти новые треки в отмеченных папках.",
    );
    q.keys(
        &[c(Char(' '))],
        "Include subfolders in regular playlists. The desk scans only its existing groups.",
        "Включить подпапки обычных плейлистов. На столе проверяются только папки его групп.",
    );
    q.keys(
        &[p(Up), p(Down)],
        "Select a folder.",
        "Выбрать папку в списке.",
    );
    q.keys(
        &[p(Home), p(End)],
        "Go to the first or last folder.",
        "Перейти к первой или последней папке.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move one page through the folder list.",
        "Перейти на страницу по списку папок.",
    );

    let q = &mut sections[C::Search as usize];
    q.keys(&[p(Tab), s(Tab)], "In Search, cycle query, results and playlist selection. In the inline filter, Tab finishes typing and keeps the filter.", "В поиске — перейти между строкой запроса, результатами и плейлистами. В фильтре — закончить ввод и оставить фильтр включенным.");
    q.keys(
        &[c(Char('a'))],
        "Select the entire search query or inline filter.",
        "Выделить весь поисковый запрос или фильтр.",
    );
    q.keys(
        &[c(Char('l'))],
        "In Search, include all playlists.",
        "В Поиске включить все плейлисты.",
    );
    q.keys(
        &[c(Char('d'))],
        "With the playlist section focused, clear all playlist marks.",
        "В списке плейлистов поиска — снять все галочки.",
    );
    q.keys(
        &[c(Char(' '))],
        "With the playlist section focused, toggle the current playlist.",
        "В списке плейлистов поиска — поставить или снять галочку.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "In Search's playlist section, select the previous or next playlist.",
        "В поиске — выбрать соседний плейлист.",
    );
    q.keys(
        &[p(Up), p(Down)],
        "Select a matching track without leaving text input.",
        "Выбрать найденный трек. Запрос можно продолжать вводить.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move by eight matching tracks in Search or the inline filter.",
        "Перейти на восемь найденных композиций в Поиске или фильтре.",
    );
    q.keys(
        &[p(Home), p(End)],
        "In Search, select the first or last result.",
        "В Поиске выбрать первый или последний результат.",
    );
    q.keys(
        &[p(Enter)],
        "Play the selected match and keep the query open.",
        "Включить найденный трек. Окно поиска останется открытым.",
    );
    q.keys(
        &[p(Escape)],
        "Close Search. In the inline filter, restore its previous value and finish editing.",
        "Закрыть поиск. В фильтре — отменить текущий ввод и вернуть прежний запрос.",
    );
    q.keys(
        &[p(Backspace)],
        "Remove a character or the selected query.",
        "Стереть символ или выделенный текст запроса.",
    );

    let q = &mut sections[C::Layout as usize];
    for key in [Left, Right, Up, Down] {
        q.keys(
            &[ca(key)],
            "Swap the focused block with the neighbor in this direction.",
            "Поменять выбранный блок с соседним в направлении стрелки.",
        );
        q.keys(
            &[cs(key)],
            "Resize the focused block in this direction.",
            "Изменить размер выбранного блока в направлении стрелки.",
        );
    }
    q.keys(
        &[ca(Char(' '))],
        "Collapse or expand the focused block.",
        "Свернуть или развернуть активный блок.",
    );
    q.keys(
        &[ca(Delete)],
        "Hide the focused block. Restore it through Blocks.",
        "Скрыть активный блок. Вернуть его можно через Блоки.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "Inside Blocks: collapse or expand the selected category.",
        "В окне Блоки: свернуть или раскрыть выбранную категорию.",
    );
    q.keys(
        &[p(Char(' '))],
        "Inside Blocks: toggle visibility of the selected block or button.",
        "В окне Блоки: показать или скрыть выбранный блок или кнопку.",
    );
    q.keys(
        &[p(Enter)],
        "Inside Blocks: activate the selected row.",
        "В окне Блоки: выбрать действие.",
    );

    let q = &mut sections[C::Settings as usize];
    q.topic("Larger text: Settings > Themes > Font and geometry > Font size. In TUI, change the font size in your terminal's settings.",
        "Увеличить буквы: Настройки > Темы > Шрифт и геометрия > Размер шрифта. В TUI размер шрифта меняется в настройках терминала.",
        "мелко плохо видно увеличить буквы масштаб зум размер текста zoom tiny text bigger letters font size");
    q.topic("Background animation: Settings > Themes > Animation. Choose a style, turn animation off or change its brightness.",
        "Анимация фона: Настройки > Темы > Анимация. Здесь можно выбрать вид, выключить анимацию или изменить ее яркость.",
        "фон мешает мигает раздражает слишком ярко выключить искры background distracting flashing disable brightness");
    q.topic("Button shortcuts and EQ/FX/VIS: Settings > General. You can hide shortcut labels or the player's special buttons.",
        "Клавиши на кнопках и EQ/FX/VIS: Настройки > Общие. Можно скрыть подписи клавиш или специальные кнопки проигрывателя.",
        "слишком много кнопок убрать буквы загроможден интерфейс special buttons clutter labels shortcuts");
    q.keys(
        &[p(Tab)],
        "Switch between settings categories and parameters.",
        "Перейти между списком разделов и их настройками.",
    );
    q.keys(
        &[p(Up), p(Down), p(Home), p(End)],
        "Select a setting or move to the first or last setting.",
        "Выбрать параметр либо перейти к первому или последнему.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move by five settings rows.",
        "Перейти на пять строк настроек.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "Change a setting. In the section list, expand or collapse Themes, Mods or Music services and choose a tab.",
        "Изменить значение настройки. В списке разделов — раскрыть или свернуть Темы, Моды или Сервисы и выбрать вкладку.",
    );
    q.keys(
        &[p(Enter), p(Char(' '))],
        "Open the selected section or change its parameter.",
        "Открыть выбранный раздел или изменить параметр.",
    );
    q.note("Themes → Animation changes the background scene and brightness. Create or update a theme preset to save them together. Older presets keep the current animation until updated.",
        "Темы → Анимация: выбор фоновой сцены и яркости. Создайте или обновите пресет темы, чтобы сохранить их вместе. Старые пресеты сохраняют текущую анимацию, пока вы их не обновите.");
    q.keys(
        &[p(Insert)],
        "Create a profile, palette or theme preset in its settings section.",
        "Создать профиль, палитру или пресет в открытом разделе.",
    );
    q.keys(
        &[p(F(3))],
        "Rename the selected profile, palette or theme preset.",
        "Переименовать выбранный профиль, палитру или пресет темы.",
    );
    q.keys(
        &[p(Delete)],
        "Request deletion of the selected profile, palette or preset; a confirmation follows.",
        "Удалить выбранный профиль, палитру или пресет после подтверждения.",
    );
    q.keys(
        &[p(F(1))],
        "Open help for the selected parameter.",
        "Открыть пояснение выбранного параметра.",
    );
    q.keys(
        &[c(Char('i'))],
        "In Palettes: import a palette from a file.",
        "В Палитрах: загрузить палитру из файла.",
    );
    q.keys(
        &[c(Char('e'))],
        "In Palettes: save a palette to a file.",
        "В Палитрах: сохранить палитру в файл.",
    );
    q.keys(
        &[c(Char('f'))],
        "In palette export: choose the destination folder.",
        "При экспорте палитры: выбрать папку назначения.",
    );
    q.note("To change a shortcut, open its action and press the new combination. Navigation keys, such as plain arrows, cannot be assigned.",
        "Чтобы изменить сочетание, откройте нужное действие и нажмите новые клавиши. Клавиши навигации, например стрелки без Ctrl, назначить нельзя.");
    q.note("In the shortcut editor, Tab or Shift+Tab switches between player keys and global controls. Arrows select a global setting; Enter or Space changes it. Esc closes the editor.",
        "В окне назначения Tab или Shift+Tab переключает клавиши плеера и глобальные настройки. Стрелки выбирают настройку, Enter или Space меняет ее. Esc закрывает окно.");

    let q = &mut sections[C::Palette as usize];
    q.keys(
        &[p(Tab), s(Tab), p(Up), p(Down)],
        "Switch between color channels and the HEX field.",
        "Выбрать ползунок цвета или поле HEX с кодом цвета.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "On a channel, change it by one step; in HEX, move the cursor.",
        "Изменить цвет на один шаг. В поле HEX — переместить курсор.",
    );
    q.keys(
        &[s(Left), s(Right)],
        "On a channel, change it by ten steps; in HEX, extend the text selection.",
        "Изменить цвет на 10 шагов. В поле HEX — выделить текст.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "On a color channel, increase or decrease by ten steps.",
        "Изменить значение ползунка цвета на 10 шагов.",
    );
    q.keys(
        &[p(Home), p(End)],
        "On a channel, set minimum or maximum; in HEX, go to the beginning or end.",
        "Поставить ползунок на минимум или максимум. В поле HEX — перейти в начало или конец.",
    );
    q.keys(
        &[c(Left), c(Right), s(Home), s(End), cs(Left), cs(Right)],
        "In HEX: Ctrl+arrows go to the ends; Shift selects text up to the new cursor position.",
        "В HEX: Ctrl+стрелка — в начало или конец. Shift — выделить текст до курсора.",
    );
    q.keys(
        &[c(Char('a'))],
        "Focus HEX and select its entire value.",
        "Выделить весь код цвета в поле HEX.",
    );
    q.keys(
        &[c(Char('c')), c(Insert)],
        "Copy the selected HEX text. Use Ctrl+A first to copy the entire color.",
        "Скопировать выделенный код цвета. Ctrl+A сначала выделит его целиком.",
    );
    q.keys(
        &[c(Char('x')), s(Delete)],
        "Cut the selected HEX text.",
        "Вырезать выделенный код цвета.",
    );
    q.keys(
        &[p(Backspace), p(Delete)],
        "In HEX, remove the selection or the neighboring character.",
        "В HEX удалить выделение или соседний символ.",
    );
    q.keys(
        &[c(Backspace), c(Delete)],
        "In HEX with no selection, remove text to the beginning or end.",
        "В HEX без выделения удалить текст до начала или конца.",
    );
    q.keys(
        &[p(Enter)],
        "Apply the edited color.",
        "Применить измененный цвет.",
    );
    q.note("Paste into HEX using your interface's paste command. In GUITUI this is Ctrl+V; terminal paste follows the terminal's settings.", "Вставить код цвета в версии с окном — Ctrl+V. В терминале используйте его клавиши вставки.");

    let q = &mut sections[C::Equalizer as usize];
    q.keys(
        &[p(Left), p(Right), p(Tab), s(Tab)],
        "Select the previous or next equalizer band, including preamp.",
        "Выбрать полосу эквалайзера или предусиление.",
    );
    q.keys(
        &[p(Up), p(Down)],
        "Change the selected band by 1 dB; in the preset list, select a preset.",
        "Изменить выбранную полосу на 1 дБ; в списке пресетов выбрать пресет.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Change a band by 3 dB; in the preset list, move by eight presets.",
        "Изменить полосу на 3 дБ; в списке пресетов перейти на восемь строк.",
    );
    q.keys(
        &[p(Home)],
        "Reset the selected band to 0 dB; in presets, go to the first preset.",
        "Вернуть выбранную полосу к 0 дБ; в пресетах перейти к первому.",
    );
    q.keys(
        &[p(End)],
        "In the preset list, select the last preset.",
        "В списке пресетов выбрать последний пресет.",
    );
    q.keys(
        &[p(Char(' '))],
        "Enable or disable the equalizer.",
        "Включить или выключить эквалайзер.",
    );
    q.keys(
        &[p(Enter)],
        "Open presets or apply the selected preset.",
        "Открыть пресеты или применить выбранный.",
    );
    q.keys(
        &[p(Char('r'))],
        "Reset equalizer gains.",
        "Сбросить усиление полос эквалайзера.",
    );
    q.note("1–9 selects one of the first nine presets; 0 selects the tenth. All presets are available through the list. Esc closes the preset list before the equalizer window.",
        "1–9 выбирает один из первых девяти пресетов, 0 — десятый. Все пресеты доступны через список. Esc сначала закрывает список пресетов, затем эквалайзер.");

    let q = &mut sections[C::Effects as usize];
    q.keys(
        &[p(Up), p(Down), p(Tab), s(Tab)],
        "Select speed, tempo, pitch or bass.",
        "Выбрать скорость, темп, тональность или бас.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "Change the selected parameter by one step.",
        "Изменить выбранный параметр на один шаг.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Increase or decrease the selected parameter by ten steps.",
        "Увеличить или уменьшить выбранный параметр на десять шагов.",
    );
    q.keys(
        &[p(Home)],
        "Restore the normal value of the selected parameter.",
        "Восстановить нормальное значение выбранного параметра.",
    );
    q.keys(
        &[p(Char(' '))],
        "Toggle reverse playback.",
        "Переключить обратное воспроизведение.",
    );
    q.keys(
        &[p(Char('r'))],
        "Reset all playback effects.",
        "Сбросить все эффекты воспроизведения.",
    );
    q.keys(
        &[p(Char('p'))],
        "Toggle saving these effects for the current track.",
        "Переключить сохранение этих эффектов для текущей композиции.",
    );

    let q = &mut sections[C::Tags as usize];
    q.note("On the desk, check several tracks to edit their tags together. The sample shows one file; only edited fields and checked cleanup choices apply to the others. File renaming and moving require a single track.",
        "Чтобы править несколько треков, отметьте их на столе. Редактор покажет теги одного из них. Ваши правки и выбранное удаление тегов применятся ко всем. Переименование и перенос доступны для одного файла.");
    q.keys(
        &[p(Tab)],
        "Switch editor tabs; inside a field editor, move to the next input.",
        "Перейти к следующей вкладке редактора. При вводе — к следующему полю.",
    );
    q.keys(
        &[s(Tab)],
        "Inside field editing, move to the previous input.",
        "При вводе — перейти к предыдущему полю.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "In the tag list, change the tag format. Inside a text field, move the cursor.",
        "В списке тегов сменить формат тегов. В текстовом поле переместить курсор.",
    );
    q.keys(
        &[p(Insert)],
        "Add a tag or cover; on the File tab, edit the file name.",
        "Добавить тег или обложку; на вкладке Файл изменить имя файла.",
    );
    q.keys(&[p(Enter)], "Open a field or save its value. When moving a file, choose a folder. In a write confirmation, save the changes.", "Открыть поле или сохранить введенное значение. При переносе — выбрать папку. В подтверждении записи — записать изменения.");
    q.keys(
        &[p(Char(' ')), p(Delete)],
        "Mark the selected field, cover or tag block for removal.",
        "Пометить тег, обложку или весь блок тегов на удаление.",
    );
    q.keys(
        &[c(Char('s'))],
        "Finish editing the field, then confirm saving changes to the selected files.",
        "Записать изменения в выбранные файлы после подтверждения. Сначала завершите ввод в поле.",
    );
    q.keys(
        &[p(Char('f'))],
        "Open the File tab; inside destination-folder selection, enter a folder path.",
        "Открыть вкладку Файл. При выборе папки для переноса — ввести путь к ней.",
    );
    q.keys(
        &[p(Char('t'))],
        "On the File tab, edit the file-name template.",
        "На вкладке Файл изменить шаблон имени.",
    );
    q.keys(
        &[p(Char('g'))],
        "On the File tab, build a name from the template.",
        "На вкладке Файл — составить имя по шаблону.",
    );
    q.keys(
        &[p(Char('m'))],
        "Choose the playlist folder to move this file to.",
        "Выбрать плейлист и папку, куда перенести выбранный файл.",
    );
    q.keys(
        &[c(Char('a'))],
        "Inside field editing, select the entire value.",
        "Выделить весь текст в поле.",
    );
    q.keys(
        &[p(F(10)), c(Char('j'))],
        "Inside a tag text field, insert a newline.",
        "Начать новую строку в тексте тега.",
    );
    q.keys(
        &[p(Home), p(End)],
        "Inside field editing, move the cursor to the beginning or end.",
        "При правке поля переместить курсор в начало или конец.",
    );
    q.keys(
        &[p(Backspace), p(Delete)],
        "Inside field editing, remove the selected text or neighboring character.",
        "При правке поля удалить выделенный текст или соседний символ.",
    );

    let q = &mut sections[C::Lyrics as usize];
    q.note("Song lyrics is a separate block. In Layout mode, drag its title or [↕], resize shared borders, or collapse it. Its position is saved with the layout. Visibility brings back a hidden lyrics block.",
        "Текст песни — отдельный блок. В режиме Блоки тяните его за заголовок или [↕], меняйте размер общей границей или сворачивайте. Положение сохраняется вместе с раскладкой. Скрытый блок можно вернуть через Видимость.");
    q.note("A word is underlined only when extended LRC provides explicit start and end markers for it. Plain text, line-only timing and incomplete word timing keep the line highlight.",
        "Слово подчеркивается только при явных метках его начала и конца в расширенном LRC. При обычной или неполной разметке остается подсветка строки.");
    q.note("The panel opens a confident match automatically. Otherwise, click a song in the panel. Variants lets you change your choice.",
        "Панель сама открывает уверенное совпадение. Иначе выберите вариант прямо в ней. Кнопка \"Варианты\" позволяет сменить выбор.");
    q.note("Click or scroll inside the panel to use its keyboard controls. ↑↓ selects a result; Page Up/Down moves by a page; Home/End goes to the start/end.",
        "Нажмите в панели или прокрутите ее колесом для управления с клавиатуры. Стрелки выбирают вариант, Page Up/Down листают страницы, Home/End переходят в начало и конец.");
    q.keys(&[p(Enter)], "In the panel: open the selected result; while reading, show variants.",
        "В панели: открыть выбранный вариант; при чтении текста — показать варианты.");
    q.keys(&[p(Escape)], "In the panel: return from variants to the lyrics, then return keyboard control to the playlist.",
        "В панели: вернуться от вариантов к тексту, затем вернуть управление клавиатурой плейлисту.");
    q.keys(
        &[p(Enter)],
        "Open the selected lyrics result.",
        "Открыть выбранный текст песни.",
    );
    for (key, en, ru) in [
        (
            'f',
            "Refine artist and title for the search.",
            "Уточнить исполнителя и название для поиска.",
        ),
        (
            'r',
            "Search again; in the sources view, refresh provider status.",
            "Искать снова; в списке источников обновить их состояние.",
        ),
        (
            's',
            "Save the lyrics in the player.",
            "Сохранить текст песни в плеере.",
        ),
        (
            'e',
            "Open the lyrics for editing.",
            "Открыть текст песни для редактирования.",
        ),
        (
            'x',
            "Write the lyrics into the audio file after confirmation.",
            "Записать текст песни в аудиофайл после подтверждения.",
        ),
        (
            'l',
            "Turn automatic lyrics scrolling on or off.",
            "Включить или выключить прокрутку текста вслед за песней.",
        ),
        (
            'n',
            "Exclude this source for the current song and search elsewhere.",
            "Искать текст этой песни в другом сервисе.",
        ),
        (
            'i',
            "Show or hide source information.",
            "Показать или скрыть сведения об источниках.",
        ),
    ] {
        q.keys(&[p(Char(key))], en, ru);
    }

    let q = &mut sections[C::Downloads as usize];
    q.topic("Where downloads go: Downloads > Settings sets the default folder. When adding a link, Choose folder opens a folder picker with sorting and filtering; Downloads saves directly to the system Downloads folder.",
        "Куда скачивается музыка: Загрузки > Настройки задают папку по умолчанию. При добавлении ссылки кнопка Выбрать папку открывает список с сортировкой и фильтром; В Загрузки сохраняет в системную папку загрузок.",
        "куда сохранилось где скачанная музыка найти скачанный файл путь сохранения saved downloaded files location destination");
    q.note("Paste a link, choose a folder and a downloader, then press Enter. A missing folder is created automatically; if unavailable, Downloads is used.",
        "Вставьте ссылку, выберите папку и способ скачивания, затем нажмите Enter. Новая папка создается автоматически; если она недоступна, файл попадет в Загрузки.");
    q.keys(&[c(Char('f'))], "In the download form: choose a folder. Type its name to filter the folder list.",
        "В форме загрузки: выбрать папку. Введите имя, чтобы отфильтровать список папок.");
    q.keys(&[c(Char('d'))], "In the download form: save directly to Downloads.",
        "В форме загрузки: сохранять прямо в Загрузки.");
    q.keys(&[c(Char('1'))], "In the download form: use yt-dlp.", "В форме загрузки: выбрать yt-dlp.");
    q.keys(&[c(Char('2'))], "In the download form: use Cobalt.", "В форме загрузки: выбрать Cobalt.");
    for (key, en, ru) in [
        ('a', "Add a download link.", "Добавить ссылку для загрузки."),
        ('f', "Open music search.", "Открыть поиск музыки."),
        (
            'e',
            "Edit a download that has not started yet.",
            "Изменить загрузку, которая еще не началась.",
        ),
        (
            's',
            "Open download settings.",
            "Открыть настройки загрузок.",
        ),
        (
            'r',
            "Retry the selected download.",
            "Повторить выбранную загрузку.",
        ),
        (
            'c',
            "Cancel the selected download.",
            "Отменить выбранную загрузку.",
        ),
    ] {
        q.keys(&[p(Char(key))], en, ru);
    }
    q.keys(
        &[p(Enter)],
        "Open the selected download or search result.",
        "Открыть выбранную загрузку или результат поиска.",
    );
    q.keys(
        &[p(Delete)],
        "Request removal of the selected download-history entry.",
        "Запросить удаление выбранной записи из истории загрузок.",
    );

    let q = &mut sections[C::Services as usize];
    q.note("First Spotify sign-in has three steps: create an app on Spotify, copy its return address, then paste Client ID. Connect opens browser sign-in automatically.",
        "Первый вход в Spotify проходит за три шага: создать приложение на сайте, скопировать адрес возврата, вставить Client ID. Кнопка \"Подключить\" сама открывает вход в браузере.");
    q.keys(&[c(Char('1'))], "In Spotify setup: open the app registration page.",
        "При подключении Spotify: открыть страницу создания приложения.");
    q.keys(&[c(Char('2'))], "In Spotify setup: copy the return address for Redirect URIs.",
        "При подключении Spotify: скопировать адрес для поля Redirect URIs.");
    q.keys(&[p(Enter)], "In Spotify setup: go to the next step; on the last step, connect the account.",
        "При подключении Spotify: перейти к следующему шагу; на последнем шаге — подключить аккаунт.");
    q.note("Manage accounts, music, playlists and devices in Settings → Music services. Tab switches between the section list and its content.",
        "Подключения, музыка, плейлисты и устройства находятся в Настройки → Музыкальные сервисы. Tab переключает список разделов и содержимое.");
    q.keys(
        &[p(Enter)],
        "Open the selected service, playlist or track.",
        "Открыть выбранный сервис, плейлист или композицию.",
    );
    q.keys(
        &[p(Char('f'))],
        "Search the selected music service.",
        "Искать в выбранном музыкальном сервисе.",
    );
    q.keys(
        &[p(Char('e'))],
        "Edit the selected connection.",
        "Изменить выбранное подключение.",
    );
    q.keys(
        &[p(Delete)],
        "Remove the connection after confirmation.",
        "Удалить выбранное подключение после подтверждения.",
    );
    q.keys(
        &[p(Char(' '))],
        "Pause or resume service playback.",
        "Приостановить или продолжить воспроизведение из сервиса.",
    );

    let q = &mut sections[C::Mods as usize];
    q.note("Manage mods in Settings → Mods → Installed. The creation guide is in the neighboring tab. Tab switches between sections and content.",
        "Управление модами: Настройки → Моды → Установленные. Руководство по созданию — в соседней вкладке. Tab переключает разделы и содержимое.");
    q.keys(
        &[p(Enter)],
        "Open the selected mod. In an installation confirmation, Enter returns without installing.",
        "Открыть выбранный мод. В подтверждении установки Enter возвращает без установки.",
    );
    for (key, en, ru) in [
        ('n', "Create a mod.", "Создать мод."),
        (
            'i',
            "Choose a mod to install; in its preview, confirm installation.",
            "Выбрать мод для установки; в его просмотре подтвердить установку.",
        ),
        ('e', "Edit mod information.", "Изменить сведения о моде."),
        ('c', "Open the mod code.", "Открыть код мода."),
        ('s', "Open mod settings.", "Открыть настройки мода."),
    ] {
        q.keys(&[p(Char(key))], en, ru);
    }
    q.keys(
        &[p(Char(' '))],
        "Enable or disable the selected mod.",
        "Включить или выключить выбранный мод.",
    );
    q.keys(
        &[p(Delete)],
        "Remove the mod after confirmation.",
        "Удалить выбранный мод после подтверждения.",
    );
    q.note(
        "1–9 runs the corresponding command of the selected mod.",
        "1–9 запускает соответствующую команду выбранного мода.",
    );

    for category in [C::Services, C::Mods] {
        sections[category as usize].keys(
            &[p(Home), p(End)],
            "Go to the beginning or end of the list or mod description.",
            "Перейти в начало или конец списка либо описания мода.",
        );
    }
    for category in [C::Tags, C::Lyrics, C::Downloads, C::Services, C::Mods] {
        let q = &mut sections[category as usize];
        q.keys(
            &[p(Up), p(Down)],
            "Move through this window's rows or text.",
            "Перемещаться по строкам или тексту этого окна.",
        );
        q.keys(&[p(PageUp), p(PageDown)], "Move by ten rows. In lyrics source information, move by the visible page.", "Перейти на десять строк. В сведениях об источниках текста песни — на видимую страницу.");
        q.keys(&[p(Escape)], "Go back or close the window. If there are unsaved changes, confirm before leaving.", "Вернуться назад или закрыть окно. Если есть несохраненные правки, плеер попросит подтвердить выход.");
    }
    for category in [C::Lyrics, C::Downloads, C::Services, C::Mods] {
        sections[category as usize].keys(
            &[p(Char('d'))],
            "Confirm deletion. Press Enter or Esc to cancel.",
            "Подтвердить удаление. Чтобы отменить, нажмите Enter или Esc.",
        );
    }

    let q = &mut sections[C::Statistics as usize];
    q.keys(
        &[p(Left), p(Right), p(Tab), s(Tab)],
        "Switch Tracks, Artists, Albums and Recent tabs.",
        "Переключить вкладки Треки, Артисты, Альбомы и Недавние.",
    );
    q.keys(
        &[p(Up), p(Down), p(Home), p(End)],
        "Select a statistics row or jump to the first or last row.",
        "Выбрать строку статистики либо перейти к первой или последней.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move by one visible page of statistics.",
        "Перейти на одну видимую страницу статистики.",
    );
    q.keys(
        &[p(Char('s'))],
        "Cycle statistics sorting, including playback starts.",
        "Сменить сортировку статистики, включая количество включений.",
    );
    q.keys(
        &[p(Char('r'))],
        "Refresh statistics.",
        "Обновить статистику.",
    );
    q.keys(
        &[p(Enter)],
        "Locate the selected track in a playlist.",
        "Перейти к выбранной композиции в плейлисте.",
    );
    q.keys(
        &[p(Escape)],
        "Close Statistics even when the main player has focus.",
        "Закрыть статистику, даже если перед этим вы нажали на плеер.",
    );

    let q = &mut sections[C::Composition as usize];
    q.keys(
        &[p(Char(' '))],
        "Mark or unmark the selected source playlist or track.",
        "Поставить или снять галочку у плейлиста или трека.",
    );
    q.keys(
        &[c(Char('a'))],
        "Select all sources or tracks in this step.",
        "Отметить все плейлисты или треки на этом шаге.",
    );
    q.keys(
        &[cs(Char('a'))],
        "Clear all selections in this step.",
        "Снять все отметки текущего шага.",
    );
    q.keys(
        &[c(Up), c(Down)],
        "Move the selected source playlist up or down in the mix order.",
        "Переставить плейлисты в нужном порядке для подборки.",
    );
    q.keys(
        &[p(Backspace)],
        "Return to the preceding composition step.",
        "Вернуться к предыдущему шагу сборки.",
    );
    q.keys(
        &[p(Enter)],
        "Continue to track selection or naming the new playlist.",
        "Перейти к выбору композиций или названию нового плейлиста.",
    );
    q.keys(
        &[p(Up), p(Down), p(Home), p(End)],
        "Select a row or jump to the beginning or end.",
        "Выбрать строку либо перейти в начало или конец.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "Move by eight rows.",
        "Перейти на восемь строк.",
    );

    let q = &mut sections[C::Text as usize];
    q.keys(
        &[c(Char('a'))],
        "Select the entire active text field.",
        "Выделить весь текст в выбранном поле.",
    );
    q.keys(
        &[p(Backspace)],
        "Delete the selected text or preceding character.",
        "Удалить выделенный текст или предыдущий символ.",
    );
    q.keys(&[p(Enter)], "In a simple name field, save. In forms, toggle a checkbox, move to the next field, or save the last field; in multiline text, insert a newline.", "При вводе имени — сохранить. В форме — поставить галочку или перейти дальше; на последнем поле — сохранить. В многострочном тексте — начать новую строку.");
    q.keys(
        &[c(Char('s'))],
        "In forms such as lyrics and download settings, save all fields.",
        "Сохранить форму целиком: например, текст песни или настройки загрузки.",
    );
    q.keys(
        &[c(Enter)],
        "In a form's text field, save the form; on a checkbox, toggle it.",
        "Сохранить форму. Если выбрана галочка — поставить или снять ее.",
    );
    q.keys(
        &[p(Tab), s(Tab)],
        "In forms, select the next or previous field.",
        "В формах выбрать следующее или предыдущее поле.",
    );
    q.keys(
        &[p(Left), p(Right), p(Home), p(End)],
        "In form inputs, move the cursor or go to the beginning or end.",
        "В полях форм переместить курсор либо перейти в начало или конец.",
    );
    q.keys(
        &[p(Delete)],
        "In form inputs, delete the following character or selection.",
        "В полях форм удалить следующий символ или выделение.",
    );
    q.keys(
        &[p(Up), p(Down), p(PageUp), p(PageDown)],
        "In multiline forms, move by one or ten text lines while keeping the column.",
        "В многострочных формах перейти на одну или десять строк с сохранением позиции в строке.",
    );
    q.keys(
        &[c(Char('v'))],
        "GUITUI: paste into the active text input. In TUI, use the terminal's paste command.",
        "В версии с окном — вставить текст в выбранное поле. В терминале используйте его клавиши вставки.",
    );
    q.keys(&[c(Char('c'))], "Copy selected text in Help search or a form. In copyable help and system-control windows, copy the displayed text. TUI: outside these inputs, windows and HEX, Ctrl+C exits the player.", "Скопировать выделенный запрос в справке или текст поля формы. В справке с копированием и окне MPRIS скопировать показанный текст. В TUI вне этих полей, окон и HEX сочетание Ctrl+C закрывает плеер.");
    q.keys(
        &[p(Enter)],
        "In copyable help and system-control windows, copy all displayed text.",
        "В справке с копированием и окне MPRIS — скопировать весь текст.",
    );
    q.keys(
        &[p(Char('d'))],
        "Leave the form without saving. Press Enter to keep editing.",
        "Выйти из формы без сохранения. Чтобы продолжить редактирование, нажмите Enter.",
    );
    q.note("Delete a playlist: Y or Д confirms; N or Н cancels. You can also choose a button with arrows or Tab, then press Enter.",
        "Удаление плейлиста: Y или Д — удалить, N или Н — отменить. Также можно выбрать кнопку стрелками или Tab и нажать Enter.");
    q.note(
        "Reset shortcuts: choose a button with arrows or Tab, then press Enter or Space.",
        "Сброс клавиш: выберите кнопку стрелками или Tab, затем нажмите Enter или Space.",
    );
    q.note("Save tags: Enter confirms writing the changes to the file. Esc cancels any confirmation window.",
        "Запись тегов: Enter подтверждает изменения в файле. Esc отменяет действие в любом окне подтверждения.");

    let q = &mut sections[C::Mouse as usize];
    for (en, ru) in [
        (
            "Wheel: scroll the list under the pointer.",
            "Колесо: прокрутить список под указателем.",
        ),
        (
            "Ctrl+wheel in lists: scroll five times faster.",
            "Ctrl+колесо в списках: прокрутить в пять раз быстрее.",
        ),
        (
            "Ctrl+Shift+wheel in lists: scroll by the visible page.",
            "Ctrl+Shift+колесо в списках: листать видимыми страницами.",
        ),
        (
            "Right-click: open the playlist, track or player menu under the pointer.",
            "Правый клик: открыть меню плейлистов, композиций или проигрывателя под указателем.",
        ),
        (
            "Double-click a track: play it. Double-click its title in the player: mark it and go to its playlist.",
            "Двойной клик по композиции: играть. По названию в проигрывателе: отметить ее и перейти к ней в плейлисте.",
        ),
        (
            "Hold a track checkbox and move across rows to mark or unmark tracks. The edge scrolls the list.",
            "Зажмите галочку композиции и ведите по строкам для отметки или снятия отметок. У края список прокручивается.",
        ),
        (
            "Click a group checkbox to mark or unmark its tracks matching the filter.",
            "Галочка группы отмечает или снимает отметки с композиций, подходящих под фильтр.",
        ),
        (
            "Double-click a group title or separator to collapse or expand it. The + and − buttons on the right need one click.",
            "Двойное нажатие по названию или линии группы сворачивает и раскрывает ее. Кнопкам + и − справа достаточно одного нажатия.",
        ),
        (
            "Drag a regular playlist row to another playlist to change its position. Order requires Edit. Wheel scrolls while dragging; Esc cancels.",
            "Перетащите строку обычного плейлиста на другой, чтобы изменить позицию. В Порядке нужна Правка. Колесо прокручивает список при переносе, Esc отменяет перенос.",
        ),
        (
            "Drag a block title or move handle: drop at an edge to place it beside another block, or in the center to swap. Drag a shared border to resize.",
            "Перетащите блок за заголовок или ручку. Бросьте у края другого блока, чтобы поставить рядом, или в центре — чтобы поменять местами. Потяните общую границу, чтобы изменить размеры.",
        ),
        (
            "Hold the timeline to preview a position; release to seek. Esc cancels. Click or drag the volume bar to change volume.",
            "Зажмите дорожку для выбора позиции; отпустите для перемотки. Esc отменяет выбор. Клик или перетаскивание полоски громкости меняет громкость.",
        ),
        (
            "Repeat button: one click toggles track repeat; double-click selects playlist repeat.",
            "Кнопка повтора: один клик переключает повтор трека, двойной выбирает повтор плейлиста.",
        ),
        (
            "On color controls, drag or use the wheel to change the channel. In HEX, drag to select text; double-click selects all.",
            "Меняйте цвет колесом или перетаскиванием ползунка. В поле HEX выделяйте текст мышью; двойной клик выделяет весь код цвета.",
        ),
        (
            "GUITUI file drop: Order sends files to the desk; Chaos sends them to the open playlist.",
            "Перетаскивание файлов в GUITUI: Порядок направляет их на стол, Хаос — в открытый плейлист.",
        ),
    ] {
        q.note(en, ru);
    }

    let q = &mut sections[C::System as usize];
    q.note("To control music from another window, set shortcuts in Settings > Keys, then add the generated bindings to your desktop settings. Media keys use MPRIS, the system media controls.",
        "Чтобы управлять плеером без переключения окна, задайте сочетания в Настройки → Клавиши. Затем добавьте готовые назначения в настройки рабочего стола. Медиаклавиши работают через MPRIS — системное управление музыкой.");
    for binding in &app.settings.preferences.global_shortcuts {
        let text = format!(
                "{} — {} ({})",
                binding.key,
                binding.action.name(language),
                language.text(
                    if binding.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    if binding.enabled {
                        "включено"
                    } else {
                        "выключено"
                    }
                )
            );
        let mut entry = Entry::new(None, text.clone(), text);
        entry.aliases = crate::help_search::aliases(binding.action);
        q.entries.push(entry);
    }
    if app.settings.preferences.global_shortcuts.is_empty() {
        q.note(
            "No global shortcuts are saved in this profile.",
            "В этом профиле глобальные сочетания еще не сохранены.",
        );
    }
    sections[C::Guide as usize].entries.extend(
        guide
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(|text| Entry::new(None, text.clone(), text)),
    );
    for (index, section) in sections.iter_mut().enumerate() {
        if configured[index] > 0 {
            if section.entries.len() > configured[index] {
                section.entries.insert(
                    configured[index],
                    Entry::heading(
                        language
                            .text(
                                "Window controls — fixed shortcuts:",
                                "Клавиши окна — не переназначаются:",
                            )
                            .into(),
                    ),
                );
            }
            section.entries.insert(
                0,
                Entry::heading(
                    language
                        .text("Your shortcuts:", "Ваши горячие клавиши:")
                        .into(),
                ),
            );
        } else if section.entries.iter().any(|entry| entry.key.is_some()) {
            section.entries.insert(
                0,
                Entry::heading(
                    language
                        .text(
                            "Window controls — fixed shortcuts:",
                            "Клавиши окна — не переназначаются:",
                        )
                        .into(),
                ),
            );
        }
    }
    sections
}
