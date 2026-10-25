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
            Self::OpenAddMenu => language.text("Open the Add music menu.", "Открыть меню добавления музыки."),
            Self::OpenPlaylistMenu => language.text("Open the Playlists menu.", "Открыть меню плейлистов."),
            Self::OpenTracksMenu => language.text("Open the Tracks menu.", "Открыть меню композиций."),
            Self::OpenApplicationMenu => language.text("Open the Almavorn menu.", "Открыть меню Almavorn."),
            Self::ToggleToolbar => language.text("Show or hide extra buttons at the top.", "Показать или скрыть дополнительные кнопки сверху."),
            Self::Mods => language.text("Open Mods in Settings to install, create or customize a mod.", "Открыть Моды в настройках: установить, создать или настроить мод."),
            Self::Services => language.text("Open Music services in Settings to connect an account and listen online.", "Открыть музыкальные сервисы в настройках: подключить аккаунт и слушать онлайн."),
            Self::Downloads => language.text("Open Downloads to find music or download a link.", "Открыть Загрузки: найти музыку или скачать по ссылке."),
            Self::Lyrics => language.text("Find and save lyrics for the playing or selected track.", "Найти и сохранить текст играющей или выбранной песни."),
            Self::Quit => language.text("Close the player and save settings and playback position.", "Закрыть плеер, сохранив настройки и место прослушивания."),
            Self::Help => language.text("Open Help and keyboard shortcuts.", "Открыть справку и список горячих клавиш."),
            Self::Settings => language.text("Open Settings.", "Открыть настройки."),
            Self::Statistics => language.text("Open listening statistics: tracks, artists, albums and time played.", "Открыть статистику: треки, исполнители, альбомы и время прослушивания."),
            Self::TogglePlay => language.text("Pause or resume. If playback is stopped, start the selected track.", "Поставить на паузу или продолжить. Если воспроизведение остановлено — включить выбранный трек."),
            Self::Stop => language.text("Stop playback and clear the playing track.", "Остановить музыку и убрать трек из проигрывателя."),
            Self::Next => language.text("Play the next track.", "Включить следующий трек."),
            Self::Previous => language.text("Play the previous track.", "Включить предыдущий трек."),
            Self::CycleRepeat => language.text("Cycle repeat: off, playlist, track. Click the repeat button once for a track or twice for the playlist. Next still skips tracks.", "Сменить повтор: выключен → плейлист → трек. На кнопке повтора один клик повторяет трек, двойной — плейлист. Следующий трек можно включить вручную."),
            Self::ToggleShuffle => language.text("Play tracks in random order. Previous retraces playback history. Track repeat takes priority.", "Включить случайный порядок. Назад возвращает к уже сыгранным трекам. Если включен повтор трека, он продолжит повторяться."),
            Self::Equalizer => language.text("Open the equalizer: 10 bands, preamp and presets. Changes apply right away and save to this profile.", "Открыть эквалайзер: 10 полос, предусиление и пресеты. Изменения сразу слышны и сохраняются в профиле."),
            Self::PlaybackTuner => language.text("Adjust speed, tempo, pitch, reverse and bass. The wheel changes sliders by 1%; settings save to this profile.", "Настроить скорость, темп, тональность, обратное воспроизведение и бас. Колесо меняет ползунки на 1%; настройки сохраняются в профиле."),
            Self::VolumeUp => language.text("Increase volume by 5%.", "Увеличить громкость на 5%."),
            Self::CycleVisualizer => language.text("Switch the background animation or turn it off. The style and brightness save to this profile.", "Сменить анимацию фона или выключить ее. Вид и яркость сохраняются в профиле."),
            Self::VolumeDown => language.text("Decrease volume by 5%.", "Уменьшить громкость на 5%."),
            Self::SeekForward => language.text("Seek forward 5 seconds if the file supports seeking.", "Перемотать вперед на 5 секунд, если файл поддерживает перемотку."),
            Self::SeekBackward => language.text("Seek back 5 seconds if the file supports seeking.", "Перемотать назад на 5 секунд, если файл поддерживает перемотку."),
            Self::SwitchMode => language.text("Switch between Order and Chaos.", "Переключить Порядок и Хаос."),
            Self::ToggleEdit => language.text("Allow changes to the selected Order playlist. Done protects it again.", "Разрешить правку выбранного плейлиста Порядка. Готово снова защищает его."),
            Self::ToggleDesk => language.text("In Chaos, send added files to the sorting desk. Order always adds them there.", "В Хаосе добавлять файлы на сортировочный стол. В Порядке они всегда попадают туда."),
            Self::AddFiles => language.text("Add files. Order uses the sorting desk; Chaos uses the open playlist unless Add to desk is checked.", "Добавить файлы. В Порядке — на стол. В Хаосе — в открытый плейлист, если выключена галочка Добавлять на стол."),
            Self::AddFolder => language.text("Add a folder to the sorting desk. Its tracks appear in a group named after the folder.", "Добавить папку на сортировочный стол. Треки появятся группой с названием папки."),
            Self::AddFolderPlaylists => language.text("Create one playlist per checked folder, using the folder names. Enable groups to include subfolders.", "Создать плейлист из каждой отмеченной папки, с ее названием. Чтобы включить подпапки, включите группы."),
            Self::AddNew => language.text("Find new tracks in the playlist's folders. On the desk, scan only its group folders.", "Найти новые треки в папках плейлиста. На столе — только в папках его групп."),
            Self::PlaylistFolders => language.text("Choose which folders to scan for new tracks in this playlist.", "Выбрать папки для поиска новых треков в этом плейлисте."),
            Self::NewPlaylist => language.text("Create an empty playlist in the current mode.", "Создать пустой плейлист в текущем режиме."),
            Self::CopyPlaylist => language.text("Copy the playlist, keeping its track order and linked folders. In Order, enable editing first.", "Скопировать плейлист вместе с порядком треков и связанными папками. В Порядке сначала включите редактирование."),
            Self::ComposePlaylists => language.text("Build a new mix from selected playlists and tracks. The original playlists stay as they are.", "Собрать новую подборку из выбранных плейлистов и треков. Исходные плейлисты сохранятся."),
            Self::Rename => language.text("Rename a playlist or track. On the desk, open the file name and tags editor.", "Переименовать плейлист или трек. На столе — открыть редактор имени файла и тегов."),
            Self::Delete => language.text("Remove a playlist or track from the list after confirmation. Files stay on disk.", "Убрать плейлист или трек из списка после подтверждения. Файлы останутся на диске."),
            Self::MoveUp => language.text("Move the selected playlist or track up one place.", "Переместить выбранный плейлист или трек на одно место выше."),
            Self::MoveDown => language.text("Move the selected playlist or track down one place.", "Переместить выбранный плейлист или трек на одно место ниже."),
            Self::PlaylistUp => language.text("Move the playlist up one place. In Order, enable editing first.", "Поднять плейлист на одну строку. В Порядке сначала включите редактирование."),
            Self::PlaylistDown => language.text("Move the playlist down one place. In Order, enable editing first.", "Опустить плейлист на одну строку. В Порядке сначала включите редактирование."),
            Self::Transfer => language.text("Copy checked tracks to another playlist that allows editing.", "Скопировать отмеченные треки в другой плейлист, открытый для правки."),
            Self::Search => language.text("Find tracks across selected playlists. Results update as you type.", "Найти треки в выбранных плейлистах. Результаты появляются по мере ввода."),
            Self::Filter => language.text("Show only matching tracks in the open playlist. Type in the Tracks header.", "Показать только подходящие треки в открытом плейлисте. Введите запрос в строке Композиций."),
            Self::Sort => language.text("Change how tracks are sorted on screen. Click a column to sort; click again to reverse. The saved track order stays unchanged.", "Изменить сортировку на экране. Клик по столбцу сортирует, повторный — меняет направление. Сохраненный порядок треков остается прежним."),
            Self::Undo => language.text("Undo a change in Chaos or on the sorting desk.", "Отменить действие в Хаосе или на сортировочном столе."),
            Self::Redo => language.text("Redo the last undone change.", "Вернуть отмененное действие."),
            Self::Metadata => language.text("View track information and saved file tags.", "Посмотреть сведения о треке и сохраненные теги файла."),
            Self::EditTags => language.text("Edit tags and the file name on the desk. You can then move the file to a playlist folder.", "Изменить теги и имя файла на столе. Затем файл можно перенести в папку плейлиста."),
            Self::Mark => language.text("Check or uncheck a track for copying or looping.", "Поставить или снять галочку у трека для копирования или повтора."),
            Self::PlayMarked => language.text("Loop only checked tracks. Check or uncheck tracks to change the loop. Unchecking all stops playback; no new playlist is created.", "Слушать по кругу только отмеченные треки. Галочки сразу меняют набор. Если снять все — музыка остановится. Новый плейлист не создается."),
            Self::PlayWholePlaylist => language.text("Leave the checked-track loop and play the whole playlist. The current track keeps playing.", "Перейти от отмеченных треков ко всему плейлисту. Текущий трек продолжит играть."),
            Self::PlaylistPanel => language.text("Move the playlist panel.", "Переместить панель плейлистов."),
            Self::PlayerPanel => language.text("Move the player panel.", "Переместить панель проигрывателя."),
            Self::Panels => language.text("Arrange blocks by dragging their titles and borders. Use the top bar to show or hide blocks and buttons.", "Настроить расположение блоков: перетащить заголовки и границы. Верхняя строка позволяет показать или скрыть блоки и кнопки."),
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
