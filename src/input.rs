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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyPress {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyPress {
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
    NewPlaylist,
    Rename,
    Delete,
    MoveUp,
    MoveDown,
    Transfer,
    Search,
    Sort,
    Undo,
    Redo,
    Metadata,
    Mark,
    PlaylistPanel,
    PlayerPanel,
}

impl Action {
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Quit => language.text("Quit", "Выход"),
            Self::Help => language.text("Help", "Помощь"),
            Self::Settings => language.text("Settings", "Настройки"),
            Self::TogglePlay => language.text("Play / pause", "Играть / пауза"),
            Self::Stop => language.text("Stop", "Стоп"),
            Self::Next => language.text("Next track", "Следующая композиция"),
            Self::Previous => language.text("Previous track", "Предыдущая композиция"),
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
            Self::NewPlaylist => language.text("New playlist", "Создать плейлист"),
            Self::Rename => language.text("Rename", "Переименовать"),
            Self::Delete => language.text("Remove", "Убрать"),
            Self::MoveUp => language.text("Move up", "Переместить выше"),
            Self::MoveDown => language.text("Move down", "Переместить ниже"),
            Self::Transfer => language.text("Copy tracks to playlist", "Копировать в плейлист"),
            Self::Search => language.text("Search", "Поиск"),
            Self::Sort => language.text("Sort view", "Сортировка вида"),
            Self::Undo => language.text("Undo", "Отменить"),
            Self::Redo => language.text("Redo", "Повторить"),
            Self::Metadata => language.text("Track information", "Сведения о композиции"),
            Self::Mark => language.text("Mark track", "Отметить композицию"),
            Self::PlaylistPanel => {
                language.text("Move playlists panel", "Переместить панель плейлистов")
            }
            Self::PlayerPanel => language.text("Move player panel", "Переместить проигрыватель"),
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
        (Undo, Key::Char('z'), true, false),
        (Redo, Key::Char('y'), true, false),
        (PlaylistPanel, Key::Char('l'), true, false),
        (PlayerPanel, Key::Char('p'), true, false),
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
    Move { x: u16, y: u16 },
    Click { x: u16, y: u16, double: bool },
    Scroll { x: u16, y: u16, delta: i16 },
}
