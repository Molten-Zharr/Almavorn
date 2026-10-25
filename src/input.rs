use crate::model::Language;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Key {
    Char(char),
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

impl Key {
    pub(crate) fn shortcut_choices() -> impl Iterator<Item = Self> {
        "abcdefghijklmnopqrstuvwxyz0123456789+-=,./;[]\\'`"
            .chars()
            .map(Self::Char)
            .chain((1..=12).map(Self::F))
            .chain([
                Self::Char(' '),
                Self::Enter,
                Self::Backspace,
                Self::Insert,
                Self::Delete,
                Self::Tab,
                Self::Up,
                Self::Down,
                Self::Left,
                Self::Right,
                Self::Home,
                Self::End,
                Self::PageUp,
                Self::PageDown,
            ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPress {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyPress {
    pub fn key_labels(self) -> Vec<String> {
        let mut labels = Vec::new();
        for (enabled, label) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ] {
            if enabled {
                labels.push(label.into());
            }
        }
        labels.push(match self.key {
            Key::Escape => "Esc".into(),
            Key::Delete => "Del".into(),
            Key::Insert => "Ins".into(),
            Key::PageUp => "PgUp".into(),
            Key::PageDown => "PgDn".into(),
            Key::Up => "↑".into(),
            Key::Down => "↓".into(),
            Key::Left => "←".into(),
            Key::Right => "→".into(),
            _ => Self::plain(self.key).label(),
        });
        labels
    }

    pub fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    pub fn label(self) -> String {
        let key = match self.key {
            Key::Char(' ') => "Space".to_owned(),
            Key::Char(c) => c.to_uppercase().to_string(),
            Key::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        };
        format!(
            "{}{}{}{}",
            if self.ctrl { "Ctrl+" } else { "" },
            if self.alt { "Alt+" } else { "" },
            if self.shift { "Shift+" } else { "" },
            key
        )
    }

    pub fn normalized(mut self) -> Self {
        if let Key::Char(c) = self.key {
            let c = c.to_lowercase().next().unwrap_or(c);
            // Горячие клавиши работают и при русской раскладке терминала.
            let russian = "йцукенгшщзхъфывапролджэячсмитьбю";
            let latin = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";
            let mapped = russian
                .chars()
                .position(|v| v == c)
                .and_then(|n| latin.chars().nth(n))
                .unwrap_or(c);
            self.key = Key::Char(mapped);
            if matches!(mapped, '+' | '-' | '/') {
                self.shift = false;
            }
        }
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    OpenAddMenu,
    OpenPlaylistMenu,
    OpenTracksMenu,
    OpenApplicationMenu,
    ToggleToolbar,
    Mods,
    Services,
    Downloads,
    Lyrics,
    Quit,
    Help,
    Settings,
    Statistics,
    TogglePlay,
    Stop,
    Next,
    Previous,
    CycleRepeat,
    ToggleShuffle,
    Equalizer,
    PlaybackTuner,
    CycleVisualizer,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBackward,
    SwitchMode,
    ToggleEdit,
    ToggleDesk,
    AddFiles,
    AddFolder,
    AddFolderPlaylists,
    AddNew,
    PlaylistFolders,
    NewPlaylist,
    CopyPlaylist,
    ComposePlaylists,
    Rename,
    Delete,
    MoveUp,
    MoveDown,
    PlaylistUp,
    PlaylistDown,
    Transfer,
    Search,
    Filter,
    Sort,
    Undo,
    Redo,
    Metadata,
    EditTags,
    Mark,
    PlayMarked,
    PlayWholePlaylist,
    PlaylistPanel,
    PlayerPanel,
    Panels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutGroup {
    Player,
    MainWindow,
    Playlist,
}

impl ShortcutGroup {
    pub const ALL: [Self; 3] = [Self::Player, Self::MainWindow, Self::Playlist];

    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Player => language.text("Player shortcuts", "Клавиши проигрывателя"),
            Self::MainWindow => language.text("Main window shortcuts", "Клавиши основного окна"),
            Self::Playlist => language.text("Playlist shortcuts", "Клавиши плейлиста"),
        }
    }
}

impl Action {
    pub fn shortcut_group(self) -> ShortcutGroup {
        match self {
            Self::TogglePlay
            | Self::Lyrics
            | Self::Stop
            | Self::Next
            | Self::Previous
            | Self::CycleRepeat
            | Self::ToggleShuffle
            | Self::Equalizer
            | Self::PlaybackTuner
            | Self::CycleVisualizer
            | Self::VolumeUp
            | Self::VolumeDown
            | Self::SeekForward
            | Self::SeekBackward => ShortcutGroup::Player,
            Self::Quit
            | Self::OpenAddMenu
            | Self::OpenPlaylistMenu
            | Self::OpenTracksMenu
            | Self::OpenApplicationMenu
            | Self::ToggleToolbar
            | Self::Help
            | Self::Settings
            | Self::Statistics
            | Self::SwitchMode
            | Self::ToggleDesk
            | Self::Search
            | Self::Undo
            | Self::Redo
            | Self::PlaylistPanel
            | Self::PlayerPanel
            | Self::Panels => ShortcutGroup::MainWindow,
            _ => ShortcutGroup::Playlist,
        }
    }

    pub fn description(self, language: Language) -> &'static str {
        match self {
            Self::OpenAddMenu => language.text("Open the menu for adding music.", "Открыть меню добавления музыки."),
            Self::OpenPlaylistMenu => language.text("Open playlist commands.", "Открыть команды плейлиста."),
            Self::OpenTracksMenu => language.text("Open track commands.", "Открыть команды композиций."),
            Self::OpenApplicationMenu => language.text("Open the application menu.", "Открыть меню приложения."),
            Self::ToggleToolbar => language.text("Expand or collapse the top toolbar.", "Развернуть или свернуть верхнюю панель."),
            Self::Mods => language.text("Install, create and customize player mods.", "Установить, создать и настроить моды плеера."),
            Self::Services => language.text("Connect music accounts, search and listen online.", "Подключить аккаунты, найти музыку и слушать онлайн."),
            Self::Downloads => language.text("Find music, download a link and manage the download queue.", "Найти музыку, скачать по ссылке и управлять очередью загрузки."),
            Self::Lyrics => language.text("Find, view and save lyrics for the current or selected song.", "Найти, посмотреть и сохранить текст текущей или выбранной песни."),
            Self::Quit => language.text("Close the application and finish saving settings.", "Закрыть приложение и завершить сохранение настроек."),
            Self::Help => language.text("Open the guide and shortcut list.", "Открыть руководство и список сочетаний."),
            Self::Settings => language.text("Open the settings pages.", "Открыть страницы настроек."),
            Self::Statistics => language.text("View listening time, track plays, artists, albums and recent listening history.", "Открыть время прослушивания, счётчики треков, исполнителей, альбомов и недавнюю историю."),
            Self::TogglePlay => language.text("Play the selected track or pause/resume playback.", "Играть выбранную композицию либо поставить воспроизведение на паузу или продолжить."),
            Self::Stop => language.text("Stop playback and clear the current track.", "Остановить воспроизведение и убрать текущую композицию."),
            Self::Next => language.text("Play the next track in the queue.", "Играть следующую композицию в очереди."),
            Self::Previous => language.text("Play the previous track in the queue.", "Играть предыдущую композицию в очереди."),
            Self::CycleRepeat => language.text("Repeat button: single click toggles track repeat; double-click enables playlist repeat. Shortcut and menu cycle off, playlist, track. Next still skips the track.", "Кнопка повтора: одиночный клик включает или выключает повтор трека; двойной включает повтор плейлиста. Горячая клавиша и меню переключают: выключен, плейлист, трек. Кнопка следующего трека позволяет его пропустить."),
            Self::ToggleShuffle => language.text("Choose a random next track from the playing playlist, excluding the current track. Previous returns through playback history. Track repeat takes priority.", "Выбирать следующий трек случайно из воспроизводимого плейлиста, исключая текущий. Назад возвращает по истории воспроизведения. Повтор трека имеет приоритет."),
            Self::Equalizer => language.text("Open the ten-band equalizer, preamp and presets. Changes apply during playback and save in the active profile.", "Открыть десятиполосный эквалайзер, предусиление и пресеты. Изменения применяются во время проигрывания и сохраняются в активный профиль."),
            Self::PlaybackTuner => language.text("Adjust speed, tempo, pitch, reverse playback and bass. The wheel changes sliders by 1%. Changes save in the active profile.", "Настроить скорость, темп, тональность, обратное проигрывание и бас. Колесо меняет ползунки по 1%. Настройки сохраняются в активном профиле."),
            Self::VolumeUp => language.text("Increase playback volume by 5%.", "Увеличить громкость на 5%."),
            Self::CycleVisualizer => language.text("Cycle music-driven background animations, including off. Style and brightness are saved in the active profile.", "Переключать музыкальные анимации фона, включая отключение. Вид и яркость сохраняются в активном профиле."),
            Self::VolumeDown => language.text("Decrease playback volume by 5%.", "Уменьшить громкость на 5%."),
            Self::SeekForward => language.text("Seek forward by five seconds if the file supports it.", "Перемотать вперёд на пять секунд, если файл поддерживает перемотку."),
            Self::SeekBackward => language.text("Seek backward by five seconds if the file supports it.", "Перемотать назад на пять секунд, если файл поддерживает перемотку."),
            Self::SwitchMode => language.text("Switch between Order and Chaos playlists.", "Переключить плейлисты Порядка и Хаоса."),
            Self::ToggleEdit => language.text("Open only the selected Order playlist for editing; Done protects it again.", "Открыть для редактирования только выбранный плейлист Порядка. Готово снова защищает его."),
            Self::ToggleDesk => language.text("In Chaos, send selected files to the desk instead of the open playlist. Order always uses the desk.", "В Хаосе направлять выбранные файлы на стол вместо открытого плейлиста. В Порядке файлы всегда идут на стол."),
            Self::AddFiles => language.text("Order adds files to the desk. Chaos adds them to the open playlist, unless Add to desk is checked.", "В Порядке файлы добавляются на стол. В Хаосе — в открытый плейлист, если не включена галочка Добавлять на стол."),
            Self::AddFolder => language.text("Add a specific folder to the sorting desk, grouped by folder.", "Добавить выбранную папку на сортировочный стол с группировкой композиций по папкам."),
            Self::AddFolderPlaylists => language.text("Check folders and create one playlist per folder, named after it. Subfolders are skipped unless groups are enabled.", "Отметить папки и создать из каждой плейлист с ее названием. Подпапки пропускаются, если не включены группы."),
            Self::AddNew => language.text("Find new tracks in this playlist's linked folders and add them to this playlist. The sorting desk is not scanned.", "Найти новые композиции в связанных папках текущего плейлиста и добавить в него. На сортировочном столе поиск отключен."),
            Self::PlaylistFolders => language.text("View and manage folders used to find new tracks for this playlist.", "Просмотреть и настроить папки для поиска новых композиций этого плейлиста."),
            Self::NewPlaylist => language.text("Create an empty playlist in the current mode.", "Создать пустой плейлист в текущем режиме."),
            Self::CopyPlaylist => language.text("Create a copy with the same stored track positions and linked folders. Order requires editing.", "Создать копию с теми же сохраненными позициями композиций и связанными папками. В Порядке требуется редактирование."),
            Self::ComposePlaylists => language.text("Choose source playlists, select tracks and arrange them in a new mix. Source playlists are kept.", "Выбрать источники, отметить композиции и задать их порядок в новой подборке. Исходные плейлисты сохраняются."),
            Self::Rename => language.text("Rename a playlist or track title. On the sorting desk, edit the file name and tags.", "Переименовать плейлист или композицию. На сортировочном столе открыть имя файла и теги."),
            Self::Delete => language.text("Confirm removal of a playlist or track entry; keep source files.", "Подтвердить удаление плейлиста либо композиции из списка; сохранить исходные файлы."),
            Self::MoveUp => language.text("Move the selection one position earlier in stored order.", "Переместить выбранный элемент на одну позицию выше в сохранённом порядке."),
            Self::MoveDown => language.text("Move the selection one position later in stored order.", "Переместить выбранный элемент на одну позицию ниже в сохранённом порядке."),
            Self::PlaylistUp => language.text("Move the selected playlist one position up. Order requires editing.", "Переместить выбранный плейлист на одну позицию выше. В Порядке требуется редактирование."),
            Self::PlaylistDown => language.text("Move the selected playlist one position down. Order requires editing.", "Переместить выбранный плейлист на одну позицию ниже. В Порядке требуется редактирование."),
            Self::Transfer => language.text("Copy selected tracks to another editable playlist.", "Копировать отмеченные композиции в другой доступный для правки плейлист."),
            Self::Search => language.text("Search selected playlists in a separate window with live results.", "Искать по выбранным плейлистам в отдельном окне с живыми результатами."),
            Self::Filter => language.text("Type in the Tracks header to filter only the current playlist.", "Вводить в строке Композиций для фильтрации только текущего плейлиста."),
            Self::Sort => language.text("Cycle view sorting; click a column header to sort, click again to reverse. Saved positions stay unchanged.", "Менять сортировку вида; клик по заголовку сортирует, повторный клик меняет направление. Позиции в базе сохраняются."),
            Self::Undo => language.text("Undo a library change in Chaos or the sorting desk.", "Отменить правку библиотеки в Хаосе либо на сортировочном столе."),
            Self::Redo => language.text("Repeat a library change that was undone.", "Повторить отменённую правку библиотеки."),
            Self::Metadata => language.text("Show saved metadata and file tags.", "Показать сохраненные сведения и теги файла."),
            Self::EditTags => language.text("Edit the file name and tags, then move the file from the sorting desk to a playlist folder.", "Изменить имя файла и теги, затем перенести файл со стола в папку плейлиста."),
            Self::Mark => language.text("Toggle the selected track's checkbox for copying or looping checked tracks.", "Изменить отметку выбранной композиции для копирования или повтора отмеченных."),
            Self::PlayMarked => language.text("Loop checked tracks in this playlist. Checkboxes update the set immediately; clearing all stops playback. No playlist is created.", "Проигрывать отмеченные композиции этого плейлиста по кругу. Галочки сразу меняют набор; снятие всех отметок останавливает воспроизведение. Новый плейлист не создается."),
            Self::PlayWholePlaylist => language.text("Leave the checked-track loop and continue the whole playing playlist without restarting the current track.", "Выйти из повтора отмеченных и продолжить весь воспроизводимый плейлист без перезапуска текущей композиции."),
            Self::PlaylistPanel => language.text("Move the playlist panel around the library.", "Изменить расположение панели плейлистов."),
            Self::PlayerPanel => language.text("Move playback controls around the library.", "Изменить расположение панели проигрывателя."),
            Self::Panels => language.text("Toggle layout editing: drag titles and borders. Visibility and button choices are available in the layout toolbar.", "Включить компоновку: тянуть заголовки и границы. Видимость блоков и кнопок настраивается в верхней строке."),
        }
    }
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::OpenAddMenu => language.text("Add music menu", "Меню добавления музыки"),
            Self::OpenPlaylistMenu => language.text("Playlist menu", "Меню плейлиста"),
            Self::OpenTracksMenu => language.text("Tracks menu", "Меню композиций"),
            Self::OpenApplicationMenu => language.text("Application menu", "Меню приложения"),
            Self::ToggleToolbar => language.text("Expand / collapse toolbar", "Развернуть / свернуть верхнюю панель"),
            Self::Mods => language.text("Mods", "Моды"),
            Self::Services => language.text("Music services", "Музыкальные сервисы"),
            Self::Downloads => language.text("Downloads", "Загрузки"),
            Self::Lyrics => language.text("Song lyrics", "Текст песни"),
            Self::Quit => language.text("Quit", "Выход"),
            Self::Help => language.text("Help", "Помощь"),
            Self::Settings => language.text("Settings", "Настройки"),
            Self::Statistics => language.text("Listening statistics", "Статистика прослушивания"),
            Self::TogglePlay => language.text("Play / pause", "Играть / пауза"),
            Self::Stop => language.text("Stop", "Стоп"),
            Self::Next => language.text("Next track", "Следующая композиция"),
            Self::Previous => language.text("Previous track", "Предыдущая композиция"),
            Self::CycleRepeat => language.text("Repeat", "Повтор"),
            Self::ToggleShuffle => language.text("Shuffle", "Случайное проигрывание"),
            Self::Equalizer => language.text("Equalizer", "Эквалайзер"),
            Self::PlaybackTuner => language.text("Playback tuner", "Настройщик проигрывания"),
            Self::VolumeUp => language.text("Volume up", "Громкость выше"),
            Self::CycleVisualizer => language.text("Background animation", "Анимация фона"),
            Self::VolumeDown => language.text("Volume down", "Громкость ниже"),
            Self::SeekForward => language.text("Seek +5 seconds", "Вперед на 5 секунд"),
            Self::SeekBackward => language.text("Seek -5 seconds", "Назад на 5 секунд"),
            Self::SwitchMode => language.text("Switch mode", "Сменить режим"),
            Self::ToggleEdit => language.text("Edit selected playlist", "Редактировать плейлист"),
            Self::ToggleDesk => {
                language.text("Add to sorting desk", "Добавлять в сортировочный стол")
            }
            Self::AddFiles => language.text("Add files", "Добавить файлы"),
            Self::AddFolder => language.text("Add folder", "Добавить папку"),
            Self::AddFolderPlaylists => language.text("Folders to playlists", "Папки в плейлисты"),
            Self::AddNew => language.text("Add new files", "Добавить новые"),
            Self::PlaylistFolders => language.text("Playlist folders", "Папки плейлиста"),
            Self::NewPlaylist => language.text("New playlist", "Создать плейлист"),
            Self::CopyPlaylist => language.text("Copy playlist", "Копия плейлиста"),
            Self::ComposePlaylists => language.text("Build mix", "Собрать подборку"),
            Self::Rename => language.text("Rename", "Переименовать"),
            Self::Delete => language.text("Remove", "Убрать"),
            Self::MoveUp => language.text("Move up", "Переместить выше"),
            Self::MoveDown => language.text("Move down", "Переместить ниже"),
            Self::PlaylistUp => language.text("Playlist up", "Плейлист выше"),
            Self::PlaylistDown => language.text("Playlist down", "Плейлист ниже"),
            Self::Transfer => language.text("Copy tracks to playlist", "Копировать в плейлист"),
            Self::Search => language.text("Search", "Поиск"),
            Self::Filter => language.text("Filter", "Фильтр"),
            Self::Sort => language.text("Sort view", "Сортировка вида"),
            Self::Undo => language.text("Undo", "Отменить"),
            Self::Redo => language.text("Redo", "Повторить"),
            Self::Metadata => language.text("Track information", "Сведения о композиции"),
            Self::EditTags => language.text("Tags and file name", "Теги и имя файла"),
            Self::Mark => language.text("Mark track", "Отметить композицию"),
            Self::PlayMarked => {
                language.text("Play checked tracks only", "Прослушивать только отмеченные")
            }
            Self::PlayWholePlaylist => {
                language.text("Play the whole playlist", "Воспроизводить весь плейлист")
            }
            Self::PlaylistPanel => {
                language.text("Move playlists panel", "Переместить панель плейлистов")
            }
            Self::PlayerPanel => language.text("Move player panel", "Переместить проигрыватель"),
            Self::Panels => language.text("Workspace blocks", "Блоки приложения"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Binding {
    pub action: Action,
    pub key: KeyPress,
}

pub fn default_bindings() -> Vec<Binding> {
    use Action::*;
    let normal = [
        (Quit, Key::Char('q')),
        (Help, Key::F(1)),
        (Settings, Key::F(2)),
        (TogglePlay, Key::Char(' ')),
        (Stop, Key::Char('s')),
        (Next, Key::Char('n')),
        (Previous, Key::Char('p')),
        (CycleRepeat, Key::Char('l')),
        (ToggleShuffle, Key::Char('h')),
        (Equalizer, Key::F(4)),
        (PlaybackTuner, Key::F(5)),
        (CycleVisualizer, Key::F(6)),
        (Statistics, Key::F(7)),
        (VolumeUp, Key::Char('+')),
        (VolumeDown, Key::Char('-')),
        (SeekForward, Key::Right),
        (SeekBackward, Key::Left),
        (SwitchMode, Key::Char('m')),
        (ToggleEdit, Key::Char('e')),
        (ToggleDesk, Key::Char('t')),
        (AddFiles, Key::Char('a')),
        (AddFolder, Key::Char('f')),
        (AddNew, Key::Char('r')),
        (NewPlaylist, Key::Char('c')),
        (Rename, Key::F(3)),
        (Delete, Key::Delete),
        (Transfer, Key::Char('x')),
        (Search, Key::Char('/')),
        (Sort, Key::Char('o')),
        (Metadata, Key::Char('i')),
        (EditTags, Key::F(8)),
        (Lyrics, Key::F(9)),
        (Mark, Key::Insert),
    ];
    let mut bindings: Vec<_> = normal
        .into_iter()
        .map(|(action, key)| Binding {
            action,
            key: KeyPress::plain(key),
        })
        .collect();
    for (action, key, ctrl, alt) in [
        (OpenAddMenu, Key::Char('1'), false, true),
        (OpenPlaylistMenu, Key::Char('2'), false, true),
        (OpenTracksMenu, Key::Char('3'), false, true),
        (OpenApplicationMenu, Key::Char('4'), false, true),
        (ToggleToolbar, Key::Char('5'), false, true),
        (Downloads, Key::Char('j'), true, false),
        (Services, Key::Char('o'), true, false),
        (Mods, Key::Char('m'), true, false),
        (MoveUp, Key::Up, false, true),
        (MoveDown, Key::Down, false, true),
        (PlaylistUp, Key::Up, true, false),
        (PlaylistDown, Key::Down, true, false),
        (Undo, Key::Char('z'), true, false),
        (Redo, Key::Char('y'), true, false),
        (PlaylistPanel, Key::Char('l'), true, false),
        (PlayerPanel, Key::Char('p'), true, false),
        (Panels, Key::Char('b'), true, false),
        (Filter, Key::Char('f'), true, false),
        (PlaylistFolders, Key::Char('r'), true, false),
        (CopyPlaylist, Key::Char('d'), true, false),
        (ComposePlaylists, Key::Char('g'), true, false),
        (PlayMarked, Key::Enter, true, false),
        (PlayWholePlaylist, Key::Char('u'), true, false),
    ] {
        bindings.push(Binding {
            action,
            key: KeyPress {
                key,
                ctrl,
                alt,
                shift: false,
            },
        });
    }
    bindings.push(Binding {
        action: AddFolderPlaylists,
        key: KeyPress {
            key: Key::Char('f'),
            ctrl: true,
            alt: false,
            shift: true,
        },
    });
    bindings
}

#[derive(Clone, Debug)]
pub enum Input {
    Key(KeyPress),
    Text(String),
    DroppedFiles {
        paths: Vec<std::path::PathBuf>,
        playlist: Option<i64>,
    },
    /// Actual pointer movement or leaving the view; do not emit on idle redraws.
    Move {
        x: u16,
        y: u16,
    },
    Click {
        x: u16,
        y: u16,
        double: bool,
    },
    SecondaryClick {
        x: u16,
        y: u16,
    },
    Drag {
        x: u16,
        y: u16,
    },
    Release {
        x: u16,
        y: u16,
    },
    CancelPointer,
    Scroll {
        x: u16,
        y: u16,
        delta: i16,
        ctrl: bool,
        shift: bool,
    },
}
