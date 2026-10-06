use crate::{
    input::Action,
    model::{Language, Mode},
};
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
    pub const ALL: [Self; 6] = [
        Self::Application,
        Self::Add,
        Self::PlaylistActions,
        Self::Playlists,
        Self::Tracks,
        Self::Player,
    ];

    const LEGACY: [Self; 11] = [
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

    fn category(self) -> Self {
        match self {
            Self::Mode => Self::Application,
            Self::History => Self::Add,
            Self::View => Self::PlaylistActions,
            Self::Playback | Self::Volume => Self::Player,
            _ => self,
        }
    }

    pub fn controls(self) -> &'static [Control] {
        use Action::*;
        use Control::{Action as Button, Mode as Page};
        match self {
            Self::Application => &[
                Page(Mode::Order),
                Page(Mode::Chaos),
                Button(ToggleEdit),
                Button(ToggleDesk),
                Button(Settings),
                Button(Help),
                Button(Quit),
            ],
            Self::Add => &[
                Button(AddFiles),
                Button(AddFolder),
                Button(AddNew),
                Button(Undo),
                Button(Redo),
            ],
            Self::PlaylistActions => &[
                Button(NewPlaylist),
                Button(Rename),
                Button(Delete),
                Button(Transfer),
                Button(MoveUp),
                Button(MoveDown),
                Button(Search),
                Button(Filter),
                Button(Sort),
                Button(Metadata),
                Button(Mark),
            ],
            Self::Player => &[
                Button(Previous),
                Button(TogglePlay),
                Button(Stop),
                Button(Next),
                Button(VolumeDown),
                Button(VolumeUp),
            ],
            _ => &[],
        }
    }

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
pub enum Control {
    Mode(Mode),
    Action(Action),
}

impl Control {
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Mode(mode) => mode.name(language),
            Self::Action(action) => action.name(language),
        }
    }
}

#[derive(Clone, Copy)]
pub enum PanelRow {
    Category(Panel),
    Control(Control),
    Reset,
}

pub fn panel_rows(expanded: &[Panel]) -> Vec<PanelRow> {
    let mut rows = Vec::new();
    for panel in Panel::ALL {
        rows.push(PanelRow::Category(panel));
        if expanded.contains(&panel) {
            rows.extend(panel.controls().iter().copied().map(PanelRow::Control));
        }
    }
    rows.push(PanelRow::Reset);
    rows
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
    fn consolidate(self, retained: &[(Panel, Panel)]) -> Option<Self> {
        match self {
            Self::Panel(panel) => retained
                .iter()
                .find(|(old, _)| *old == panel)
                .map(|(_, category)| Self::Panel(*category)),
            Self::Split {
                axis,
                ratio,
                first,
                second,
            } => match (first.consolidate(retained), second.consolidate(retained)) {
                (Some(first), Some(second)) => Some(Self::split(axis, ratio, first, second)),
                (first, second) => first.or(second),
            },
        }
    }
    fn only_buttons(&self) -> bool {
        match self {
            Self::Panel(panel) => {
                !matches!(panel, Panel::Playlists | Panel::Tracks | Panel::Player)
            }
            Self::Split { first, second, .. } => first.only_buttons() && second.only_buttons(),
        }
    }

    fn compact_buttons(&mut self) {
        if let Self::Split {
            axis,
            ratio,
            first,
            second,
        } = self
        {
            if *axis == Axis::Vertical {
                if first.only_buttons() && !second.only_buttons() {
                    *ratio = (u32::from(*ratio) * 3 / 5).clamp(1, 999) as u16;
                } else if !first.only_buttons() && second.only_buttons() {
                    *ratio =
                        1000 - (u32::from(1000u16.saturating_sub(*ratio)) * 3 / 5).max(1) as u16;
                }
            }
            first.compact_buttons();
            second.compact_buttons();
        }
    }
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceLayout {
    // Versions 0/1 used separate blocks for controls now grouped by subject.
    #[serde(default)]
    pub version: u8,
    pub root: Option<Dock>,
    pub hidden: Vec<Panel>,
    pub collapsed: Vec<Panel>,
    pub hidden_controls: Vec<Control>,
}

impl Default for WorkspaceLayout {
    fn default() -> Self {
        Self {
            version: 2,
            root: None,
            hidden: Vec::new(),
            collapsed: Vec::new(),
            hidden_controls: Vec::new(),
        }
    }
}

impl WorkspaceLayout {
    pub fn normalize(&mut self) {
        if self.version == 0
            && let Some(root) = &mut self.root
        {
            root.compact_buttons();
        }
        if self.version < 2 {
            if let Some(root) = self.root.take() {
                let mut panels = Vec::new();
                root.panels(&mut panels);
                let retained: Vec<_> = Panel::ALL
                    .iter()
                    .filter_map(|category| {
                        let preferred =
                            if *category == Panel::Application && panels.contains(&Panel::Mode) {
                                Panel::Mode
                            } else {
                                *category
                            };
                        panels
                            .iter()
                            .find(|panel| **panel == preferred)
                            .or_else(|| panels.iter().find(|panel| panel.category() == *category))
                            .map(|panel| (*panel, *category))
                    })
                    .collect();
                self.root = root.consolidate(&retained);
            }
            let hidden = self.hidden.clone();
            let collapsed = self.collapsed.clone();
            for panel in &hidden {
                if Panel::LEGACY
                    .iter()
                    .filter(|member| member.category() == panel.category())
                    .all(|member| hidden.contains(member))
                {
                    continue;
                }
                use Action::*;
                use Control::{Action as Button, Mode as Page};
                let controls: &[Control] = match panel {
                    Panel::Mode => &[
                        Page(Mode::Order),
                        Page(Mode::Chaos),
                        Button(ToggleEdit),
                        Button(ToggleDesk),
                    ],
                    Panel::Application => &[Button(Settings), Button(Help), Button(Quit)],
                    Panel::Add => &[Button(AddFiles), Button(AddFolder), Button(AddNew)],
                    Panel::History => &[Button(Undo), Button(Redo)],
                    Panel::PlaylistActions => &[
                        Button(NewPlaylist),
                        Button(Rename),
                        Button(Delete),
                        Button(Transfer),
                        Button(MoveUp),
                        Button(MoveDown),
                    ],
                    Panel::View => &[Button(Search), Button(Sort), Button(Metadata), Button(Mark)],
                    Panel::Playback => &[
                        Button(Previous),
                        Button(TogglePlay),
                        Button(Stop),
                        Button(Next),
                    ],
                    Panel::Volume => &[Button(VolumeDown), Button(VolumeUp)],
                    _ => &[],
                };
                self.hidden_controls.extend_from_slice(controls);
            }
            self.hidden = Panel::ALL
                .into_iter()
                .filter(|category| {
                    Panel::LEGACY
                        .iter()
                        .filter(|panel| panel.category() == *category)
                        .all(|panel| hidden.contains(panel))
                })
                .collect();
            self.collapsed = Panel::ALL
                .into_iter()
                .filter(|category| {
                    let mut visible = Panel::LEGACY
                        .iter()
                        .filter(|panel| panel.category() == *category && !hidden.contains(panel))
                        .peekable();
                    visible.peek().is_some() && visible.all(|panel| collapsed.contains(panel))
                })
                .collect();
            self.version = 2;
        }
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
        self.hidden.retain(|panel| Panel::ALL.contains(panel));
        self.collapsed.retain(|panel| Panel::ALL.contains(panel));
        self.hidden
            .sort_by_key(|panel| Panel::ALL.iter().position(|value| value == panel));
        self.hidden.dedup();
        self.collapsed
            .sort_by_key(|panel| Panel::ALL.iter().position(|value| value == panel));
        self.collapsed.dedup();
        let controls: Vec<_> = Panel::ALL
            .iter()
            .flat_map(|panel| panel.controls())
            .copied()
            .collect();
        self.hidden_controls
            .retain(|control| controls.contains(control));
        self.hidden_controls
            .sort_by_key(|control| controls.iter().position(|value| value == control));
        self.hidden_controls.dedup();
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
    Playlist { id: i64, target: Option<i64> },
    Move { panel: Panel, origin: Position },
    Resize { split: SplitArea, before: Dock },
    Seek(Rect),
    PlaybackVolume(Rect),
    Volume(usize, Rect),
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
