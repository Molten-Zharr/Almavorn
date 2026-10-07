use super::SettingsPage;
use crate::{
    input::{Action, Key},
    model::{Language, Mode},
};
use ratatui::layout::Rect;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Focus {
    Playlists,
    #[default]
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
    CommandMenu(super::CommandMenu),
    CommandRow(usize),
    ManagePanels,
    PanelFocus(crate::workspace::Panel),
    PanelMove(crate::workspace::Panel),
    PanelCollapse(crate::workspace::Panel),
    PanelClose(crate::workspace::Panel),
    PanelVisibility(crate::workspace::Panel),
    PanelRow(usize),
    PanelResize(usize),
    ResetWorkspace,
    Action(Action),
    Mode(Mode),
    Playlist(i64),
    Track(i64),
    Mark(i64),
    SortColumn(Sort),
    FilterInput,
    ClearFilter,
    SearchInput,
    SearchResult(usize),
    SearchPlay,
    SearchPlaylist(i64),
    SearchAll(bool),
    QueryKeyboard,
    Seek(Rect),
    PlaybackVolume(Rect),
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
    PlaylistAutoName,
    CreateFolderGroups,
    TransferRow(usize),
    FolderRow(usize),
    FolderAdd,
    FolderEdit,
    FolderRemove,
    FolderScan,
    ScanSubfolders,
    ComposeRow(usize),
    ComposeMove(i64),
    DialogScroll(i16),
    Setting(usize),
    SettingsPage(SettingsPage),
    SettingsTab(SettingsPage),
    SettingsVolume(usize, Rect),
    SettingsTimeline(usize, crate::preferences::PlaybackTimeline),
    SettingSelect(usize),
    SettingAdjust(usize, i64),
    SettingHelp(usize),
    BindingModifier(usize),
    BindingKey(Key),
    EqualizerToggle,
    EqualizerPresets,
    EqualizerPreset(usize),
    EqualizerSelect(usize),
    EqualizerGain {
        index: usize,
        area: Rect,
        vertical: bool,
    },
    EqualizerReset,
    TunerSelect(usize),
    TunerSlider(usize, Rect),
    TunerReverse,
    TunerReset,
}

pub struct Hit {
    pub area: Rect,
    pub target: Target,
    pub enabled: bool,
}

pub struct Keycap {
    pub area: Rect,
    pub label: String,
}
