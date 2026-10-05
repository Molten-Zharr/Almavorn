use crate::model::Language;
use ratatui::layout::{Position, Rect};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Panel {
    Mode,
    Add,
    PlaylistActions,
    View,
    History,
    Application,
    Playlists,
    Tracks,
    Player,
    Playback,
    Volume,
}

impl Panel {
    pub const ALL: [Self; 11] = [
        Self::Mode,
        Self::Add,
        Self::PlaylistActions,
        Self::View,
        Self::History,
        Self::Application,
        Self::Playlists,
        Self::Tracks,
        Self::Player,
        Self::Playback,
        Self::Volume,
    ];

    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Mode => language.text("Mode", "Режим"),
            Self::Add => language.text("Add music", "Добавление"),
            Self::PlaylistActions => language.text("Playlist commands", "Команды плейлистов"),
            Self::View => language.text("View", "Просмотр"),
            Self::History => language.text("History", "История"),
            Self::Application => language.text("Application", "Приложение"),
            Self::Playlists => language.text("Playlists", "Плейлисты"),
            Self::Tracks => language.text("Tracks", "Композиции"),
            Self::Player => language.text("Player", "Проигрыватель"),
            Self::Playback => language.text("Playback", "Управление"),
            Self::Volume => language.text("Volume", "Громкость"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dock {
    Panel(Panel),
    Split {
        axis: Axis,
        ratio: u16,
        first: Box<Dock>,
        second: Box<Dock>,
    },
}

impl Dock {
    pub fn split(axis: Axis, ratio: u16, first: Self, second: Self) -> Self {
        Self::Split {
            axis,
            ratio: ratio.clamp(1, 999),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn panels(&self, result: &mut Vec<Panel>) {
        match self {
            Self::Panel(panel) => result.push(*panel),
            Self::Split { first, second, .. } => {
                first.panels(result);
                second.panels(result);
            }
        }
    }

    pub fn swap(&mut self, left: Panel, right: Panel) {
        match self {
            Self::Panel(panel) => {
                if *panel == left {
                    *panel = right;
                } else if *panel == right {
                    *panel = left;
                }
            }
            Self::Split { first, second, .. } => {
                first.swap(left, right);
                second.swap(left, right);
            }
        }
    }

    pub fn remove(self, panel: Panel) -> Option<Self> {
        match self {
            Self::Panel(value) => (value != panel).then_some(Self::Panel(value)),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(panel), second.remove(panel)) {
                (Some(first), Some(second)) => Some(Self::split(axis, ratio, first, second)),
                (first, second) => first.or(second),
            },
        }
    }

    pub fn insert(&mut self, panel: Panel, target: Panel, edge: Edge) {
        match self {
            Self::Panel(value) if *value == target => {
                let source = Self::Panel(panel);
                let destination = Self::Panel(target);
                *self = if matches!(edge, Edge::Left | Edge::Top) {
                    Self::split(edge.axis(), 500, source, destination)
                } else {
                    Self::split(edge.axis(), 500, destination, source)
                };
            }
            Self::Split { first, second, .. } => {
                first.insert(panel, target, edge);
                second.insert(panel, target, edge);
            }
            _ => {}
        }
    }

    pub fn at_mut(&mut self, path: &[bool]) -> Option<&mut Self> {
        if let Some((next, rest)) = path.split_first() {
            if let Self::Split { first, second, .. } = self {
                return if *next {
                    second.at_mut(rest)
                } else {
                    first.at_mut(rest)
                };
            }
            None
        } else {
            Some(self)
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceLayout {
    pub root: Option<Dock>,
    pub hidden: Vec<Panel>,
    pub collapsed: Vec<Panel>,
}

impl WorkspaceLayout {
    pub fn normalize(&mut self) {
        if let Some(root) = &self.root {
            let mut panels = Vec::new();
            root.panels(&mut panels);
            if panels.len() != Panel::ALL.len()
                || Panel::ALL
                    .iter()
                    .any(|panel| panels.iter().filter(|value| *value == panel).count() != 1)
            {
                self.root = None;
            }
        }
        self.hidden
            .sort_by_key(|panel| Panel::ALL.iter().position(|value| value == panel));
        self.hidden.dedup();
        self.collapsed
            .sort_by_key(|panel| Panel::ALL.iter().position(|value| value == panel));
        self.collapsed.dedup();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}
impl Edge {
    pub fn axis(self) -> Axis {
        if matches!(self, Self::Left | Self::Right) {
            Axis::Horizontal
        } else {
            Axis::Vertical
        }
    }
}

#[derive(Clone)]
pub struct SplitArea {
    pub path: Vec<bool>,
    pub area: Rect,
    pub axis: Axis,
    pub cut: u16,
    pub minimum: (u16, u16),
}

pub enum Gesture {
    Move { panel: Panel, origin: Position },
    Resize { split: SplitArea, before: Dock },
    Seek(Rect),
}

pub struct Workspace {
    pub bounds: Rect,
    pub root: Option<Dock>,
    pub areas: Vec<(Panel, Rect)>,
    pub splits: Vec<SplitArea>,
    pub focus: Panel,
    pub gesture: Option<Gesture>,
    pub drop: Option<(Panel, Edge)>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            bounds: Rect::default(),
            root: None,
            areas: Vec::new(),
            splits: Vec::new(),
            focus: Panel::Tracks,
            gesture: None,
            drop: None,
        }
    }
}

impl Workspace {
    pub fn destination(&self, point: Position, source: Panel) -> Option<(Panel, Edge)> {
        let (panel, area) = self
            .areas
            .iter()
            .find(|(panel, area)| *panel != source && area.contains(point))?;
        let x = point.x.saturating_sub(area.x);
        let y = point.y.saturating_sub(area.y);
        let edge = if x < area.width / 4 {
            Edge::Left
        } else if u32::from(x) >= u32::from(area.width) * 3 / 4 {
            Edge::Right
        } else if y < area.height / 4 {
            Edge::Top
        } else if u32::from(y) >= u32::from(area.height) * 3 / 4 {
            Edge::Bottom
        } else {
            Edge::Center
        };
        Some((*panel, edge))
    }
}
