use super::{
    Dialog, Focus, Hit, Keycap, Sort,
    background::{Background, LatestJob},
    browser,
    database::{DatabaseJob, LibraryJob},
    import::ImportJob,
    playback,
};
use crate::{
    audio::Audio,
    model::{Entry, Language, Mode, Playlist, Settings, Track},
    store::Change,
};
use ratatui::layout::{Position, Rect};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
};

#[derive(Default)]
pub struct LibraryState {
    pub(super) playlists: Vec<Playlist>,
    pub selected_playlist: Option<i64>,
    pub selected_entry: Option<i64>,
    pub marked: HashSet<i64>,
    pub sort: Sort,
    pub sort_descending: bool,
    pub query: String,
    pub(super) import: Option<ImportJob>,
    pub(super) database: Option<DatabaseJob>,
    pub(super) load: Option<LibraryJob>,
    pub(super) revision: u64,
    pub(super) rows_cache: std::cell::RefCell<Option<super::library::RowsCache>>,
    pub(super) pending_selection: Option<(i64, Mode)>,
    pub(super) histories: HashMap<&'static str, (Vec<Change>, Vec<Change>)>,
}

#[derive(Default)]
pub struct PlaybackState {
    pub current: Option<Track>,
    pub playing_playlist: Option<i64>,
    pub playing_entry: Option<i64>,
    pub waveform: Option<Vec<f32>>,
    pub(super) waveform_job: Option<playback::WaveformJob>,
    pub(super) pending_waveform: Option<(PathBuf, u64)>,
    pub(super) queue: Arc<[Entry]>,
    pub(super) queue_index: usize,
    pub(super) shuffle_history: VecDeque<usize>,
    pub(super) audio: Option<Audio>,
    pub(super) preparation: LatestJob<playback::PlaybackRequest, crate::audio::PreparedAudio>,
    pub(super) seek: LatestJob<playback::SeekRequest, crate::audio::PreparedAudio>,
    pub(super) seek_preview: Option<u64>,
    pub(super) seek_paused: Option<bool>,
    pub(super) preparation_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) seek_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) reverse_cache: Option<(PathBuf, Arc<crate::audio::PcmCache>)>,
}

#[derive(Default)]
pub struct UiState {
    pub(crate) settings: super::settings::SettingsView,
    pub focus: Focus,
    pub workspace: crate::workspace::Workspace,
    pub dialog: Option<Dialog>,
    pub hits: Vec<Hit>,
    pub keycaps: Vec<Keycap>,
    pub graphical_keycaps: bool,
    pub waveform_area: Rect,
    pub progress_area: Rect,
    pub playlist_area: Rect,
    pub tracks_area: Rect,
    pub playlist_offset: usize,
    pub track_offset: usize,
    pub notice: String,
    pub notice_error: bool,
    pub quit: bool,
    pub editing: bool,
    pub layout_editing: bool,
    pub toolbar_selected: Option<usize>,
    pub(crate) toolbar_areas: Vec<Rect>,
    pub(crate) dialog_scroll_max: usize,
    pub filter_editing: bool,
    pub filter_keyboard: bool,
    pub(crate) filter_field: Rect,
    pub(crate) filter_keyboard_language: Language,
    pub(crate) filter_keyboard_upper: bool,
    pub(super) filter_before: String,
    pub(crate) filter_selected_all: bool,
    pub filter_keyboard_area: Rect,
    pub overlay_area: Rect,
    pub(super) pointer: Position,
}

#[derive(Default)]
pub(super) struct BrowserState {
    pub(super) job: LatestJob<browser::BrowserRequest, super::Browser>,
    pub(super) ready: bool,
}

#[derive(Default)]
pub(super) struct SettingsPersistence {
    pub(super) file: Option<super::settings::SettingsFileJob>,
    pub(super) job: Option<Background<()>>,
    pub(super) pending: Option<Settings>,
    pub(super) failed: bool,
}

impl LibraryState {
    pub(super) fn new(playlists: Vec<Playlist>, mode: Mode, revision: u64) -> Self {
        let selected = playlists.iter().find(|playlist| playlist.mode == mode);
        let selected_playlist = selected.map(|playlist| playlist.id);
        let selected_entry = selected
            .and_then(|playlist| playlist.entries.first())
            .map(|entry| entry.id);
        Self {
            playlists,
            selected_playlist,
            selected_entry,
            revision,
            ..Default::default()
        }
    }
}

impl UiState {
    pub(super) fn new(language: Language) -> Self {
        Self {
            notice: language
                .text(
                    "Ready. Add music or create a playlist.",
                    "Готово. Добавьте музыку или создайте плейлист.",
                )
                .into(),
            ..Default::default()
        }
    }
}
