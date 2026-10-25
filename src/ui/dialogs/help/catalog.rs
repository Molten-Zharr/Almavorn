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
    (Category::Services, "Streaming services", "Стриминг"),
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
    (Category::Guide, "Player guide", "Принципы плеера"),
];

pub(super) struct Section {
    pub name: &'static str,
    pub entries: Vec<(Option<KeyPress>, String)>,
    language: Language,
}

impl Section {
    fn keys(&mut self, keys: &[KeyPress], en: &'static str, ru: &'static str) {
        for &key in keys {
            self.entries
                .push((Some(key), self.language.text(en, ru).into()));
        }
    }
    fn note(&mut self, en: &'static str, ru: &'static str) {
        self.entries.push((None, self.language.text(en, ru).into()));
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
            entries: Vec::new(),
            language,
        })
        .collect();
    for binding in &app.settings.preferences.bindings {
        for category in categories(binding.action) {
            sections[*category as usize].entries.push((
                Some(binding.key),
                format!(
                    "{}\n{}",
                    binding.action.name(language),
                    binding.action.description(language)
                ),
            ));
        }
    }
    let configured: Vec<_> = sections
        .iter()
        .map(|section| section.entries.len())
        .collect();

    let q = &mut sections[C::Navigation as usize];
    q.note("Configurable shortcuts above follow the active profile. Keys inside dialogs are listed in their own sections. Configurable letter commands in the main window also work with the Russian keyboard layout.",
        "Переназначаемые сочетания выше взяты из текущего профиля. Клавиши внутри окон описаны в своих разделах. Настраиваемые буквенные команды основного окна работают и в русской раскладке.");
    q.keys(&[p(Escape)], "Close the open dialog first, even after clicking the player. In a file picker, first clear its filter; in an editor, return or confirm discarding the draft.", "Сначала закрыть открытое окно, даже после клика по плееру. В файловом окне сначала очищается фильтр; в редакторе — возврат или подтверждение отмены черновика.");
    q.keys(
        &[p(F(10))],
        "Open the active panel's menu. Right-click opens the same menu.",
        "Открыть меню активной панели. Правый клик открывает то же меню.",
    );
    q.keys(&[p(Tab)], "Move to the next panel or the top command bar. In a menu on the bar, return to panel navigation.", "Перейти к следующей панели или верхней строке команд. Из открытого меню строки вернуться к навигации по панелям.");
    q.keys(
        &[s(Tab)],
        "Move to the previous panel or the command bar.",
        "Перейти к предыдущей панели или строке команд.",
    );
    q.keys(&[p(Up), p(Down)], "Select a row. On the command bar, select a button; in the player, change volume by 1%.", "Выбрать строку. В строке команд — выбрать кнопку; в проигрывателе — изменить громкость на 1%.");
    q.keys(&[p(Left), p(Right)], "On the command bar or in its menu, switch buttons or menus. In Help, jump to the previous or next category.", "В строке команд и ее меню перейти к соседней кнопке или меню. В помощи перейти к предыдущей или следующей категории.");
    q.keys(&[p(Enter)], "In playlists, focus tracks; in tracks, play the selection. In menus, run the selected command.", "В плейлистах перейти к композициям; в композициях — играть выбранную. В меню выполнить выбранную команду.");
    q.keys(
        &[p(Char(' '))],
        "On the command bar, open the selected button's menu.",
        "В строке команд открыть меню выбранной кнопки.",
    );
    q.keys(&[p(PageUp), p(PageDown)], "In the main library, move by 10 rows. In Help, scroll by the visible page. Other dialogs describe their own step below.", "В основной библиотеке перейти на 10 строк. В помощи листать видимыми страницами. Шаг других окон указан в их разделах.");
    q.keys(
        &[p(Home), p(End)],
        "Go to the beginning or end of the library list, command bar or Help.",
        "Перейти в начало или конец списка библиотеки, строки команд или помощи.",
    );
    q.keys(&[p(Tab), s(Tab)], "In Help: next or previous category. The category list also supports clicks and the mouse wheel.", "В помощи: следующая или предыдущая категория. Список категорий также поддерживает клик и колесо мыши.");

    let q = &mut sections[C::Playlists as usize];
    q.note("To move playlists, select a regular playlist. Order requires Edit; Chaos allows changes directly. The desk remains first. Dedicated playlist-move shortcuts work even when tracks have focus.",
        "Для перестановки выберите обычный плейлист. В Порядке сначала включите Редактировать; в Хаосе правка доступна сразу. Стол закреплен первым. Отдельные клавиши перемещения плейлистов работают и при фокусе на композициях.");
    q.keys(
        &[c(Char('d'))],
        "Inside Create playlist: toggle creation of a real folder for the new playlist.",
        "В окне создания плейлиста: включить или выключить создание настоящей папки.",
    );
    q.keys(&[c(Char('f'))], "Inside Create playlist: choose music folders; when Create playlist folder is enabled, choose where to create it.", "В окне создания плейлиста: выбрать папки с музыкой; при включенном создании папки — выбрать место ее создания.");
    q.keys(
        &[c(Char('g'))],
        "After choosing a source folder: include subfolders as track groups.",
        "После выбора исходной папки: включить ее подпапки как группы композиций.",
    );
    q.keys(&[p(Insert)], "In Playlist folders: link another folder. Available for editable regular playlists.", "В Папках плейлиста: добавить связанную папку. Доступно для обычного плейлиста, открытого для правки.");
    q.keys(&[p(F(3)), p(Enter)], "In Playlist folders: change the selected folder path. Desk group paths follow their tracks and cannot be edited here.", "В Папках плейлиста: изменить выбранный путь. Пути групп стола определяются композициями и здесь не редактируются.");
    q.keys(
        &[p(Delete)],
        "In Playlist folders: unlink the selected folder, keeping its tracks in the playlist.",
        "В Папках плейлиста: отключить выбранную папку, сохранив ее композиции в плейлисте.",
    );
    q.keys(&[p(Char('r'))], "In Playlist folders: find new tracks in this playlist's folders or the desk's current group folders.", "В Папках плейлиста: найти новые треки в его папках или текущих папках групп стола.");
    q.keys(
        &[p(Char(' '))],
        "In a regular playlist's folder window: toggle searching subfolders.",
        "В окне папок обычного плейлиста: включить или выключить поиск в подпапках.",
    );

    sections[C::Tracks as usize].note("Check tracks before copying, looping or batch tag editing. A group heading marks its visible tracks. Order requires Edit for changing playlist contents; the desk is always editable.",
        "Отметьте треки перед копированием, повтором или массовой правкой тегов. Заголовок группы отмечает ее видимые композиции. Для изменения состава плейлиста Порядка включите Редактировать; стол доступен для правки всегда.");

    let q = &mut sections[C::Adding as usize];
    q.note("The configured Add new shortcut scans the current playlist or desk groups in the main window. Inside the Add menu, the same shortcut opens folder selection.",
        "Назначенное сочетание Добавить новые в основном окне проверяет текущий плейлист или группы стола. В открытом меню добавления это же сочетание открывает выбор папок.");
    q.note("The shortcuts below work in file and folder pickers. Add files follows the mode; Add folder makes desk groups. Folders to playlists and Create playlist > From music folders share multiple-folder selection.",
        "Клавиши ниже работают в выборе файлов и папок. Добавить файлы учитывает режим; Добавить папку создает группы на столе. Папки в плейлисты и Создать плейлист → Из папок с музыкой поддерживают выбор нескольких папок.");
    q.keys(
        &[p(Enter)],
        "Open the highlighted folder or toggle a file's mark.",
        "Открыть выделенную папку или переключить отметку файла.",
    );
    q.keys(&[p(Insert), c(Char(' '))], "Mark or unmark a file or playlist folder. In Add folder, Ctrl+Space toggles subfolder scanning.", "Отметить файл или папку для плейлиста. В Добавить папку сочетание Ctrl+Space переключает поиск в подпапках.");
    q.keys(&[p(Char(' '))], "With an empty filter, toggle a mark; in Add folder, toggle subfolders. While typing a filter, insert a space.", "При пустом фильтре переключить отметку; в Добавить папку — подпапки. В непустой фильтр вводится пробел.");
    q.keys(&[c(Char('a'))], "Mark all selectable entries in the filtered folder view; previously marked folders remain selected.", "Отметить все доступные строки текущего отфильтрованного каталога; прежние отметки сохраняются.");
    q.keys(
        &[c(Char('d'))],
        "Clear all file or folder marks, including those in other folders.",
        "Снять все отметки файлов или папок, включая отметки в других каталогах.",
    );
    q.keys(&[c(Enter)], "Add marked files or create playlists from marked folders. With no marks in a single-folder picker, accept the highlighted or open folder.", "Добавить отмеченные файлы или создать плейлисты из отмеченных папок. Без отметок при выборе одной папки — принять выделенную или открытую папку.");
    q.keys(&[cs(Enter)], "In a music folder picker, choose the currently open folder, independently of the highlighted child folder.", "В выборе папки с музыкой принять открытую папку независимо от выделенной подпапки.");
    q.keys(
        &[a(Up)],
        "Open the parent folder.",
        "Открыть родительскую папку.",
    );
    q.keys(
        &[p(Backspace)],
        "Remove a filter character; when the filter is empty, open the parent folder.",
        "Убрать символ фильтра; при пустом фильтре открыть родительскую папку.",
    );
    q.keys(
        &[p(Escape)],
        "Clear a nonempty filter; press again to close the picker.",
        "Очистить непустой фильтр; повторное нажатие закрывает окно.",
    );
    q.keys(
        &[c(Char('s'))],
        "Cycle sorting: name, modification time, size, type.",
        "Сменить сортировку: имя, изменение, размер, тип.",
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
    q.note("Find new dialog: Space/Insert marks a folder; Ctrl+A marks all, Ctrl+D clears all. Enter starts scanning. Ctrl+Space toggles subfolders for regular playlists. Up/Down, Home/End and PgUp/PgDn navigate; pages follow the visible list height. Desk scans only its existing group folders.",
        "Окно Добавить новые: Space/Insert отмечает папку; Ctrl+A отмечает все, Ctrl+D снимает отметки. Enter запускает поиск. Ctrl+Space переключает подпапки обычных плейлистов. Стрелки, Home/End и PgUp/PgDn перемещают по списку; страницы соответствуют высоте списка. На столе проверяются только существующие группы.");

    let q = &mut sections[C::Search as usize];
    q.keys(&[p(Tab), s(Tab)], "In Search, cycle query, results and playlist selection. In the inline filter, Tab finishes typing and keeps the filter.", "В Поиске переключить запрос, результаты и плейлисты. Во встроенном фильтре Tab завершает ввод, сохраняя фильтр.");
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
        "При фокусе на списке плейлистов снять все отметки.",
    );
    q.keys(
        &[c(Char(' '))],
        "With the playlist section focused, toggle the current playlist.",
        "При фокусе на списке плейлистов переключить отметку текущего плейлиста.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "In Search's playlist section, select the previous or next playlist.",
        "В разделе плейлистов Поиска выбрать предыдущий или следующий плейлист.",
    );
    q.keys(
        &[p(Up), p(Down)],
        "Select a matching track without leaving text input.",
        "Выбрать найденную композицию, сохраняя возможность ввода запроса.",
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
        "Играть выбранную найденную композицию, сохранив открытый запрос.",
    );
    q.keys(
        &[p(Escape)],
        "Close Search. In the inline filter, restore its previous value and finish editing.",
        "Закрыть Поиск. Во встроенном фильтре восстановить прежнее значение и завершить ввод.",
    );
    q.keys(
        &[p(Backspace)],
        "Remove a character or the selected query.",
        "Убрать символ или выделенный запрос.",
    );

    let q = &mut sections[C::Layout as usize];
    for key in [Left, Right, Up, Down] {
        q.keys(
            &[ca(key)],
            "Swap the focused block with the neighbor in this direction.",
            "Поменять активный блок с соседним в указанном направлении.",
        );
        q.keys(
            &[cs(key)],
            "Resize the focused block in this direction.",
            "Изменить размер активного блока в указанном направлении.",
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
        "В окне Блоки: переключить видимость выбранного блока или кнопки.",
    );
    q.keys(
        &[p(Enter)],
        "Inside Blocks: activate the selected row.",
        "В окне Блоки: выполнить действие выбранной строки.",
    );

    let q = &mut sections[C::Settings as usize];
    q.keys(
        &[p(Tab)],
        "Switch between settings categories and parameters.",
        "Переключить категории настроек и параметры.",
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
        "Change a parameter; in the category list, collapse or expand Themes and select its tabs.",
        "Изменить параметр; в категориях свернуть или раскрыть Темы и перейти к их вкладкам.",
    );
    q.keys(
        &[p(Enter), p(Char(' '))],
        "Open the selected section or change its parameter.",
        "Открыть выбранный раздел или изменить параметр.",
    );
    q.keys(
        &[p(Insert)],
        "Create a profile, palette or theme preset in its settings section.",
        "Создать профиль, палитру или пресет темы в соответствующем разделе настроек.",
    );
    q.keys(
        &[p(F(3))],
        "Rename the selected profile, palette or theme preset.",
        "Переименовать выбранный профиль, палитру или пресет темы.",
    );
    q.keys(
        &[p(Delete)],
        "Request deletion of the selected profile, palette or preset; a confirmation follows.",
        "Запросить удаление выбранного профиля, палитры или пресета с подтверждением.",
    );
    q.keys(
        &[p(F(1))],
        "Open help for the selected parameter.",
        "Открыть пояснение выбранного параметра.",
    );
    q.keys(
        &[c(Char('i')), c(Char('e'))],
        "In Palettes: import or export a palette, respectively.",
        "В Палитрах: импортировать или экспортировать палитру соответственно.",
    );
    q.keys(
        &[c(Char('f'))],
        "In palette export: choose the destination folder.",
        "При экспорте палитры: выбрать папку назначения.",
    );
    q.note("Assigning a shortcut: press the desired combination. Tab/Shift+Tab switches the local shortcut and global controls; arrows select a global control and Enter/Space activates it. Esc closes. Plain navigation keys remain reserved.",
        "Назначение сочетания: нажмите нужную комбинацию. Tab/Shift+Tab переключает локальное сочетание и глобальные настройки; стрелки выбирают глобальную настройку, Enter/Space включает ее. Esc закрывает окно. Обычные клавиши навигации остаются зарезервированы.");

    let q = &mut sections[C::Palette as usize];
    q.keys(
        &[p(Tab), s(Tab), p(Up), p(Down)],
        "Switch between color channels and the HEX field.",
        "Переключить цветовые каналы и поле HEX.",
    );
    q.keys(
        &[p(Left), p(Right)],
        "On a channel, change it by one step; in HEX, move the cursor.",
        "На канале изменить значение на шаг; в HEX переместить курсор.",
    );
    q.keys(
        &[s(Left), s(Right)],
        "On a channel, change it by ten steps; in HEX, extend the text selection.",
        "На канале изменить значение на десять шагов; в HEX расширить выделение текста.",
    );
    q.keys(
        &[p(PageUp), p(PageDown)],
        "On a color channel, increase or decrease by ten steps.",
        "На цветовом канале увеличить или уменьшить значение на десять шагов.",
    );
    q.keys(
        &[p(Home), p(End)],
        "On a channel, set minimum or maximum; in HEX, go to the beginning or end.",
        "На канале установить минимум или максимум; в HEX перейти в начало или конец.",
    );
    q.keys(
        &[c(Left), c(Right), s(Home), s(End), cs(Left), cs(Right)],
        "In HEX: Ctrl+arrows go to the ends; Shift selects text up to the new cursor position.",
        "В HEX: Ctrl+стрелки переходят к краям; Shift выделяет текст до новой позиции курсора.",
    );
    q.keys(
        &[c(Char('a'))],
        "Focus HEX and select its entire value.",
        "Перейти в HEX и выделить все значение.",
    );
    q.keys(
        &[c(Char('c')), c(Insert)],
        "Copy the selected HEX text. Use Ctrl+A first to copy the entire color.",
        "Скопировать выделенный HEX-текст. Чтобы скопировать весь цвет, сначала нажмите Ctrl+A.",
    );
    q.keys(
        &[c(Char('x')), s(Delete)],
        "Cut the selected HEX text.",
        "Вырезать выделенный HEX-текст.",
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
    q.note("Paste into HEX using your interface's paste command. In GUITUI this is Ctrl+V; terminal paste follows the terminal's settings.", "Вставляйте HEX через команду вставки вашего интерфейса. В GUITUI это Ctrl+V; в терминале сочетание задает терминал.");

    let q = &mut sections[C::Equalizer as usize];
    q.keys(
        &[p(Left), p(Right), p(Tab), s(Tab)],
        "Select the previous or next equalizer band, including preamp.",
        "Выбрать предыдущую или следующую полосу эквалайзера, включая предусиление.",
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
        "На столе отметьте несколько треков для массовой правки тегов. Образец показывает один файл; к остальным применяются только измененные поля и выбранная очистка. Для переименования и переноса файла выберите один трек.");
    q.keys(
        &[p(Tab)],
        "Switch editor tabs; inside a field editor, move to the next input.",
        "Переключить вкладки редактора; внутри правки поля перейти к следующему вводу.",
    );
    q.keys(
        &[s(Tab)],
        "Inside field editing, move to the previous input.",
        "Внутри правки поля перейти к предыдущему вводу.",
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
    q.keys(&[p(Enter)], "Edit the selected field, apply a field draft, select a destination, or confirm the pending file operation.", "Изменить выбранное поле, применить черновик поля, выбрать назначение или подтвердить подготовленную запись файла.");
    q.keys(
        &[p(Char(' ')), p(Delete)],
        "Mark the selected field, cover or tag block for removal.",
        "Отметить выбранное поле, обложку или блок тегов для удаления.",
    );
    q.keys(&[c(Char('s'))], "Outside a field input, open confirmation to write all selected tag changes to the file or checked files.", "Вне ввода поля открыть подтверждение записи выбранных изменений в файл или отмеченные файлы.");
    q.keys(
        &[p(Char('f'))],
        "Open the File tab; inside destination-folder selection, enter a folder path.",
        "Открыть вкладку Файл; при выборе папки назначения ввести путь.",
    );
    q.keys(
        &[p(Char('t'))],
        "On the File tab, edit the file-name template.",
        "На вкладке Файл изменить шаблон имени.",
    );
    q.keys(
        &[p(Char('g'))],
        "On the File tab, apply the template to the name draft.",
        "На вкладке Файл применить шаблон к черновику имени.",
    );
    q.keys(
        &[p(Char('m'))],
        "Choose a playlist and folder for physically moving the single selected file.",
        "Выбрать плейлист и папку для физического переноса одного выбранного файла.",
    );
    q.keys(
        &[c(Char('a'))],
        "Inside field editing, select the entire value.",
        "При правке поля выделить все значение.",
    );
    q.keys(
        &[p(F(10)), c(Char('j'))],
        "Inside a tag text field, insert a newline.",
        "В текстовом поле тега вставить перевод строки.",
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
    q.keys(
        &[p(Enter)],
        "Open the selected lyrics result.",
        "Открыть выбранный найденный текст.",
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
            "Save the lyrics locally.",
            "Сохранить текст песни локально.",
        ),
        (
            'e',
            "Edit the lyrics draft.",
            "Изменить черновик текста песни.",
        ),
        (
            'x',
            "Write the lyrics into the audio file after confirmation.",
            "Записать текст песни в аудиофайл после подтверждения.",
        ),
        (
            'l',
            "Toggle following timed lyrics.",
            "Переключить слежение за синхронизированным текстом.",
        ),
        (
            'n',
            "Exclude this source for the current song and search elsewhere.",
            "Исключить этот источник для текущей песни и искать в другом.",
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
    for (key, en, ru) in [
        ('a', "Add a download link.", "Добавить ссылку для загрузки."),
        ('f', "Open music search.", "Открыть поиск музыки."),
        (
            'e',
            "Edit the selected pending download.",
            "Изменить выбранную ожидающую загрузку.",
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
        "Request removal of the selected connection.",
        "Запросить удаление выбранного подключения.",
    );
    q.keys(
        &[p(Char(' '))],
        "Pause or resume service playback.",
        "Приостановить или продолжить воспроизведение из сервиса.",
    );

    let q = &mut sections[C::Mods as usize];
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
        "Request removal of the selected mod.",
        "Запросить удаление выбранного мода.",
    );
    q.note(
        "1–9 runs the corresponding command of the selected mod.",
        "1–9 запускает соответствующую команду выбранного мода.",
    );

    for category in [C::Tags, C::Lyrics, C::Downloads, C::Services, C::Mods] {
        let q = &mut sections[category as usize];
        q.keys(
            &[p(Up), p(Down)],
            "Move through this window's rows or text.",
            "Перемещаться по строкам или тексту этого окна.",
        );
        q.keys(&[p(PageUp), p(PageDown)], "Move by ten rows. In lyrics source information, move by the visible page.", "Перейти на десять строк. В сведениях об источниках текста песни — на видимую страницу.");
        q.keys(&[p(Escape)], "Close the current subview or return. Unsaved changes may require a discard confirmation.", "Закрыть текущий вложенный экран или вернуться. Для несохраненных изменений может потребоваться подтверждение отмены.");
    }
    for category in [C::Lyrics, C::Downloads, C::Services, C::Mods] {
        sections[category as usize].keys(
            &[p(Char('d'))],
            "In a removal confirmation, confirm deletion. Enter or Esc cancels it.",
            "В подтверждении удаления подтвердить действие. Enter или Esc отменяет его.",
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
        "Закрыть Статистику даже при фокусе на основном плеере.",
    );

    let q = &mut sections[C::Composition as usize];
    q.keys(
        &[p(Char(' '))],
        "Mark or unmark the selected source playlist or track.",
        "Отметить или снять отметку выбранного исходного плейлиста или композиции.",
    );
    q.keys(
        &[c(Char('a'))],
        "Select all sources or tracks in this step.",
        "Выбрать все источники или композиции текущего шага.",
    );
    q.keys(
        &[cs(Char('a'))],
        "Clear all selections in this step.",
        "Снять все отметки текущего шага.",
    );
    q.keys(
        &[c(Up), c(Down)],
        "Move the selected source playlist up or down in the mix order.",
        "Переместить выбранный исходный плейлист вверх или вниз в порядке сборки.",
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
        "Выделить все активное текстовое поле.",
    );
    q.keys(
        &[p(Backspace)],
        "Delete the selected text or preceding character.",
        "Удалить выделенный текст или предыдущий символ.",
    );
    q.keys(&[p(Enter)], "In a simple name field, save. In forms, toggle a checkbox, move to the next field, or save the last field; in multiline text, insert a newline.", "В простом поле имени сохранить. В формах переключить галочку, перейти к следующему полю или сохранить последнее; в многострочном тексте вставить перевод строки.");
    q.keys(
        &[c(Char('s'))],
        "In forms such as lyrics and download settings, save all fields.",
        "В формах, например текста песни и настроек загрузки, сохранить все поля.",
    );
    q.keys(
        &[c(Enter)],
        "In a form's text field, save the form; on a checkbox, toggle it.",
        "В текстовом поле формы сохранить форму; на галочке переключить ее.",
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
        "GUITUI: вставить текст в активное поле. В TUI используйте команду вставки терминала.",
    );
    q.keys(&[c(Char('c'))], "Copyable help and system-control windows: copy the text. In TUI outside these windows or HEX, Ctrl+C exits the player.", "В копируемых справках и окне системного управления скопировать текст. В TUI вне этих окон и HEX сочетание Ctrl+C закрывает плеер.");
    q.keys(
        &[p(Enter)],
        "In copyable help and system-control windows, copy all displayed text.",
        "В копируемой справке и окне системного управления скопировать весь показанный текст.",
    );
    q.keys(
        &[p(Char('d'))],
        "In a form's discard confirmation, discard its draft; Enter returns to editing.",
        "В подтверждении отмены формы удалить ее черновик; Enter возвращает к редактированию.",
    );
    q.note("Playlist removal: Y or Д confirms; N or Н cancels. Arrows/Tab choose an answer, Enter accepts it. Resetting shortcuts also uses arrows/Tab and Enter/Space. Esc cancels confirmations. Tag-file confirmations use Enter to confirm the displayed operation.",
        "Удаление плейлиста: Y или Д подтверждает, N или Н отменяет. Стрелки/Tab выбирают ответ, Enter принимает его. Сброс клавиш также использует стрелки/Tab и Enter/Space. Esc отменяет подтверждение. При записи тегов Enter подтверждает показанную операцию.");

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
            "Click a group heading to mark its visible tracks; click again to unmark them.",
            "Клик по заголовку группы отмечает видимые композиции; повторный клик снимает отметки.",
        ),
        (
            "Drag a regular playlist row to another playlist to change its position. Order requires Edit. Wheel scrolls while dragging; Esc cancels.",
            "Перетащите строку обычного плейлиста на другой, чтобы изменить позицию. В Порядке нужна Правка. Колесо прокручивает список при переносе, Esc отменяет перенос.",
        ),
        (
            "Drag a block title or move handle: drop at an edge to place it beside another block, or in the center to swap. Drag a shared border to resize.",
            "Тяните заголовок или ручку блока: край размещает его рядом с другим, центр меняет блоки местами. Общая граница меняет размеры.",
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
            "На цветовых ползунках перетаскивание или колесо меняет канал. В HEX протягивание выделяет текст, двойной клик выделяет все.",
        ),
        (
            "GUITUI file drop: Order sends files to the desk; Chaos sends them to the open playlist.",
            "Перетаскивание файлов в GUITUI: Порядок направляет их на стол, Хаос — в открытый плейлист.",
        ),
    ] {
        q.note(en, ru);
    }

    let q = &mut sections[C::System as usize];
    q.note("Global shortcuts are set in Settings > Keys. Apply the generated bindings in your desktop environment. Media keys use the environment's MPRIS player selection.",
        "Глобальные сочетания задаются в Настройки → Клавиши. Примените готовые назначения в окружении рабочего стола. Мультимедийные клавиши используют выбранный окружением плеер MPRIS.");
    for binding in &app.settings.preferences.global_shortcuts {
        q.entries.push((
            None,
            format!(
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
            ),
        ));
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
            .map(|text| (None, text)),
    );
    for (index, section) in sections.iter_mut().enumerate() {
        if configured[index] > 0 {
            if section.entries.len() > configured[index] {
                section.entries.insert(
                    configured[index],
                    (
                        None,
                        language
                            .text(
                                "Fixed keys and controls in this section:",
                                "Фиксированные клавиши и управление в этом разделе:",
                            )
                            .into(),
                    ),
                );
            }
            section.entries.insert(
                0,
                (
                    None,
                    language
                        .text(
                            "Configurable shortcuts from the active profile:",
                            "Настраиваемые сочетания из текущего профиля:",
                        )
                        .into(),
                ),
            );
        } else if section.entries.iter().any(|(key, _)| key.is_some()) {
            section.entries.insert(
                0,
                (
                    None,
                    language
                        .text(
                            "Fixed keys inside this window:",
                            "Фиксированные клавиши внутри этого окна:",
                        )
                        .into(),
                ),
            );
        }
    }
    sections
}
