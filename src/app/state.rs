use crate::{
    input::Action,
    model::{Language, Mode},
};
use ratatui::layout::Rect;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Playlists,
    Tracks,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Position,
    Title,
    Artist,
    Album,
    Duration,
}

impl Sort {
    pub fn next(self) -> Self {
        match self {
            Self::Position => Self::Title,
            Self::Title => Self::Artist,
            Self::Artist => Self::Album,
            Self::Album => Self::Duration,
            Self::Duration => Self::Position,
        }
    }
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Position => language.text("Position", "Позиция"),
            Self::Title => language.text("Title", "Название"),
            Self::Artist => language.text("Artist", "Исполнитель"),
            Self::Album => language.text("Album", "Альбом"),
            Self::Duration => language.text("Duration", "Длительность"),
        }
    }
}

#[derive(Clone)]
pub enum Target {
    Action(Action),
    Mode(Mode),
    Playlist(i64),
    Track(i64),
    Mark(i64),
    Seek(Rect),
    CloseDialog,
    Submit,
    Text(char),
    Backspace,
    KeyboardLanguage,
    KeyboardCase,
    BrowserRow(usize),
    BrowserParent,
    BrowserMarkAll,
    BrowserOpen,
    BrowserAdd,
    TransferRow(usize),
    DialogScroll(i16),
    Setting(usize),
}

pub struct Hit {
    pub area: Rect,
    pub target: Target,
    pub enabled: bool,
}
