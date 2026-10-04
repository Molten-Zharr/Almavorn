use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    English,
    #[default]
    Russian,
}

impl Language {
    pub fn text(self, english: &'static str, russian: &'static str) -> &'static str {
        match self {
            Self::English => english,
            Self::Russian => russian,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Order,
    Chaos,
}

impl Mode {
    pub const ALL: [Self; 2] = [Self::Order, Self::Chaos];

    pub fn index(self) -> usize {
        match self {
            Self::Order => 0,
            Self::Chaos => 1,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Order => "order",
            Self::Chaos => "chaos",
        }
    }

    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Order => language.text("Order", "Порядок"),
            Self::Chaos => language.text("Chaos", "Хаос"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaylistKind {
    Normal,
    SortingDesk,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub tags: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: i64,
    pub position: i64,
    pub track: Track,
}

#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub mode: Mode,
    pub kind: PlaylistKind,
    pub position: i64,
    pub entries: Vec<Entry>,
}

impl Playlist {
    pub fn display_name(&self, language: Language) -> &str {
        match self.kind {
            PlaylistKind::Normal => &self.name,
            PlaylistKind::SortingDesk => language.text("Sorting desk", "Сортировочный стол"),
        }
    }

    pub fn can_edit(&self, order_unlocked: bool) -> bool {
        self.mode != Mode::Order || self.kind == PlaylistKind::SortingDesk || order_unlocked
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Placement {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl Placement {
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Left => language.text("Left", "Слева"),
            Self::Right => language.text("Right", "Справа"),
            Self::Top => language.text("Top", "Сверху"),
            Self::Bottom => language.text("Bottom", "Снизу"),
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Top,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Left,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Theme {
    pub accent: [u8; 3],
    pub light: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub mode: Mode,
    pub volume: f32,
    pub sorting_desk: bool,
    pub playlist_placement: Placement,
    pub player_placement: Placement,
    pub themes: [Theme; 2],
    pub bindings: Vec<crate::input::Binding>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::Russian,
            mode: Mode::Order,
            volume: 0.7,
            sorting_desk: true,
            playlist_placement: Placement::Left,
            player_placement: Placement::Bottom,
            themes: [
                Theme {
                    accent: [232, 158, 74],
                    light: false,
                },
                Theme {
                    accent: [188, 125, 228],
                    light: false,
                },
            ],
            bindings: crate::input::default_bindings(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportedTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub tags: String,
}

pub fn duration_text(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
