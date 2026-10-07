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
    Quit,
    Help,
    Settings,
    TogglePlay,
    Stop,
    Next,
    Previous,
    CycleRepeat,
    ToggleShuffle,
    VolumeUp,
    VolumeDown,
    SeekForward,
    SeekBackward,
    SwitchMode,
    ToggleEdit,
    ToggleDesk,
    AddFiles,
    AddFolder,
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
    Mark,
    PlaylistPanel,
    PlayerPanel,
    Panels,
}

impl Action {
    pub fn description(self, language: Language) -> &'static str {
        match self {
            Self::Quit => language.text("Close the application and finish saving settings.", "Закрыть приложение и завершить сохранение настроек."),
            Self::Help => language.text("Open the guide and shortcut list.", "Открыть руководство и список сочетаний."),
            Self::Settings => language.text("Open the settings pages.", "Открыть страницы настроек."),
            Self::TogglePlay => language.text("Play the selected track or pause/resume playback.", "Играть выбранную композицию либо поставить воспроизведение на паузу или продолжить."),
            Self::Stop => language.text("Stop playback and clear the current track.", "Остановить воспроизведение и убрать текущую композицию."),
            Self::Next => language.text("Play the next track in the queue.", "Играть следующую композицию в очереди."),
            Self::Previous => language.text("Play the previous track in the queue.", "Играть предыдущую композицию в очереди."),
            Self::CycleRepeat => language.text("Cycle repeat: off, playlist, track. Track repeat applies when a track ends; Next still skips it.", "Переключить повтор: выключен, плейлист, трек. Повтор трека действует по его завершении; кнопка следующего трека позволяет его пропустить."),
            Self::ToggleShuffle => language.text("Choose a random next track from the playing playlist, excluding the current track. Previous returns through playback history. Track repeat takes priority.", "Выбирать следующий трек случайно из воспроизводимого плейлиста, исключая текущий. Назад возвращает по истории воспроизведения. Повтор трека имеет приоритет."),
            Self::VolumeUp => language.text("Increase playback volume by 5%.", "Увеличить громкость на 5%."),
            Self::VolumeDown => language.text("Decrease playback volume by 5%.", "Уменьшить громкость на 5%."),
            Self::SeekForward => language.text("Seek forward by five seconds if the file supports it.", "Перемотать вперёд на пять секунд, если файл поддерживает перемотку."),
            Self::SeekBackward => language.text("Seek backward by five seconds if the file supports it.", "Перемотать назад на пять секунд, если файл поддерживает перемотку."),
            Self::SwitchMode => language.text("Switch between Order and Chaos playlists.", "Переключить плейлисты Порядка и Хаоса."),
            Self::ToggleEdit => language.text("Unlock or protect changes to Order playlists.", "Разрешить либо запретить изменение плейлистов Порядка."),
            Self::ToggleDesk => language.text("Route imported files to the desk or selected playlist.", "Направлять добавляемые файлы на стол либо в выбранный плейлист."),
            Self::AddFiles => language.text("Select music files to import.", "Выбрать музыкальные файлы для добавления."),
            Self::AddFolder => language.text("Select a folder and import its music.", "Выбрать папку и добавить её музыку."),
            Self::AddNew => language.text("Scan saved playlist folders, including subfolders, for new tracks. Existing tracks and positions stay unchanged.", "Проверить сохраненные папки плейлиста и вложенные папки на новые композиции. Существующие композиции и позиции сохраняются."),
            Self::PlaylistFolders => language.text("View and manage folders used to find new tracks for this playlist.", "Просмотреть и настроить папки для поиска новых композиций этого плейлиста."),
            Self::NewPlaylist => language.text("Create an empty playlist in the current mode.", "Создать пустой плейлист в текущем режиме."),
            Self::CopyPlaylist => language.text("Create a copy with the same stored track positions and linked folders. Order requires editing.", "Создать копию с теми же сохраненными позициями композиций и связанными папками. В Порядке требуется редактирование."),
            Self::ComposePlaylists => language.text("Build a new playlist from several lists in the chosen order, skipping repeated tracks. Original playlists are kept.", "Собрать новый плейлист из нескольких в выбранном порядке, пропуская повторные композиции. Исходные плейлисты сохраняются."),
            Self::Rename => language.text("Rename the playlist or the local track title.", "Переименовать плейлист либо название композиции в базе."),
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
            Self::Metadata => language.text("Show saved metadata and original imported tags.", "Показать сохранённые сведения и исходные импортированные теги."),
            Self::Mark => language.text("Toggle the selected track's copy checkbox.", "Изменить отметку выбранной композиции для копирования."),
            Self::PlaylistPanel => language.text("Move the playlist panel around the library.", "Изменить расположение панели плейлистов."),
            Self::PlayerPanel => language.text("Move playback controls around the library.", "Изменить расположение панели проигрывателя."),
            Self::Panels => language.text("Toggle layout editing: drag titles and borders. Visibility and button choices are available in the layout toolbar.", "Включить компоновку: тянуть заголовки и границы. Видимость блоков и кнопок настраивается в верхней строке."),
        }
    }
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Quit => language.text("Quit", "Выход"),
            Self::Help => language.text("Help", "Помощь"),
            Self::Settings => language.text("Settings", "Настройки"),
            Self::TogglePlay => language.text("Play / pause", "Играть / пауза"),
            Self::Stop => language.text("Stop", "Стоп"),
            Self::Next => language.text("Next track", "Следующая композиция"),
            Self::Previous => language.text("Previous track", "Предыдущая композиция"),
            Self::CycleRepeat => language.text("Repeat", "Повтор"),
            Self::ToggleShuffle => language.text("Shuffle", "Случайное проигрывание"),
            Self::VolumeUp => language.text("Volume up", "Громкость выше"),
            Self::VolumeDown => language.text("Volume down", "Громкость ниже"),
            Self::SeekForward => language.text("Seek +5 seconds", "Вперед на 5 секунд"),
            Self::SeekBackward => language.text("Seek -5 seconds", "Назад на 5 секунд"),
            Self::SwitchMode => language.text("Switch mode", "Сменить режим"),
            Self::ToggleEdit => language.text("Order editing", "Редактирование Порядка"),
            Self::ToggleDesk => {
                language.text("Add to sorting desk", "Добавлять в сортировочный стол")
            }
            Self::AddFiles => language.text("Add files", "Добавить файлы"),
            Self::AddFolder => language.text("Add folder", "Добавить папку"),
            Self::AddNew => language.text("Add new files", "Добавить новые"),
            Self::PlaylistFolders => language.text("Playlist folders", "Папки плейлиста"),
            Self::NewPlaylist => language.text("New playlist", "Создать плейлист"),
            Self::CopyPlaylist => language.text("Copy playlist", "Копия плейлиста"),
            Self::ComposePlaylists => language.text("Compose playlists", "Собрать плейлист"),
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
            Self::Mark => language.text("Mark track", "Отметить композицию"),
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
    bindings
}

#[derive(Clone, Debug)]
pub enum Input {
    Key(KeyPress),
    Text(String),
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
    },
}
