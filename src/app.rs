mod actions;
mod artwork;
mod background;
mod browser;
mod color_picker;
mod commands;
mod context;
mod database;
mod dialogs;
mod downloads;
mod equalizer;
mod events;
mod forms;
mod history;
mod import;
mod library;
mod lyrics;
#[cfg(target_os = "linux")]
mod mpris;
mod options;
mod playback;
mod playback_queue;
mod playlist_operations;
mod remote;
mod search;
mod scan_folders;
mod selection;
mod session;
mod settings;
mod state;
mod statistics;
mod services;
mod mods;
mod tags;
mod tuner;
mod visualizer;
mod workspace;

pub use browser::{Browser, BrowserEntry, PaletteFilePicker, PlaylistFolderPicker};
pub use commands::{CommandItem, CommandMenu};
use context::{BrowserState, SettingsPersistence};
pub use context::{LibraryState, PlaybackState, UiState};
pub use dialogs::{Dialog, TextDialog, TextPurpose};
pub use forms::{Form,FormAction,FormField,FormPurpose};
pub use downloads::{DownloadsState,DownloadsView,DownloadAction};
pub use lyrics::{LyricsState,LyricsView,LyricsAction, PanelAction as LyricsPanelAction};
pub use options::Options;
pub use search::{SearchDialog, SearchFocus};
pub use scan_folders::{ScanFolders, ScanFolderAction};
pub(crate) use settings::{SettingControl, SettingGroup};
pub use settings::{SettingsCatalog, SettingsEdit, SettingsFocus, SettingsPage};
pub use state::{Focus, Hit, Keycap, Sort, Target};
pub use mods::{ModsState,ModsView,ModAction};
pub use services::{ServicesState,ServicesView,ServiceAction,ServicePage};
pub use statistics::{StatisticsPage, StatisticsSort, StatisticsState, StatisticsView};
pub use tags::{FileInput, MovePicker, TagAction, TagEdit, TagEditor, TagsTab};

use crate::{audio::Audio, model::Settings, store::Store};
use anyhow::{Context, Result};
use std::{path::Path, sync::atomic::Ordering, time::Duration};
use tokio::runtime::{Builder, Runtime};

pub struct App {
    pub store: Store,
    runtime: Option<Runtime>,
    pub settings: Settings,
    pub library: LibraryState,
    pub playback: PlaybackState,
    pub statistics: StatisticsState,
    pub lyrics: LyricsState,
    pub downloads: DownloadsState,
    pub services: ServicesState,
    pub mods: ModsState,
    pub view: UiState,
    artwork: artwork::ArtworkState,
    browser_state: BrowserState,
    persistence: SettingsPersistence,
    session: session::SessionPersistence,
    tags: tags::TagsState,
    search_job: background::LatestJob<search::SearchRequest, Vec<search::SearchHit>>,
    command_program: std::path::PathBuf,
    #[cfg(unix)]
    remote: Option<crate::remote::Server>,
    #[cfg(target_os = "linux")]
    mpris: Option<crate::mpris::Bridge>,
}

impl App {
    fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("runtime is alive while the interface runs")
    }

    pub fn new(directory: &Path) -> Result<Self> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
            .thread_name("almavorn-worker")
            .enable_all()
            .build()
            .context("Cannot start background workers")?;
        let store = Store::open(directory)?;
        let mut settings = store.settings()?;
        settings.preferences.normalize();
        for tuner in settings.track_tuners.values_mut() {
            tuner.normalize();
        }
        let library = LibraryState::new(store.playlists()?, settings.mode, store.revision());
        let view = UiState::default();
        let mut app = Self {
            store,
            runtime: Some(runtime),
            settings,
            library,
            view,
            artwork: Default::default(),
            playback: Default::default(),
            statistics: Default::default(),
            lyrics: Default::default(),
            downloads: Default::default(),
            services: Default::default(),
            mods: Default::default(),
            browser_state: Default::default(),
            persistence: Default::default(),
            session: Default::default(),
            tags: Default::default(),
            search_job: Default::default(),
            command_program: std::env::current_exe()
                .context("Cannot locate Almavorn executable")?,
            #[cfg(unix)]
            remote: None,
            #[cfg(target_os = "linux")]
            mpris: None,
        };
        app.initialize_downloads();
        app.initialize_mods();
        if let Err(error) = app.restore_session() {
            app.failure(error);
        }
        Ok(app)
    }

    pub fn text(&self, english: &'static str, russian: &'static str) -> &'static str {
        self.settings.preferences.language.text(english, russian)
    }

    fn message(&mut self, text: String) {
        self.view.notice = text;
        self.view.notice_error = false;
        self.view.notice_until = Some(std::time::Instant::now() + Duration::from_secs(4));
    }

    fn failure(&mut self, error: anyhow::Error) {
        let reason = crate::errors::message(&error, self.settings.preferences.language);
        self.view.notice = format!(
            "{}: {}",
            self.text("Action failed", "Действие не выполнено"),
            reason.replace('\n', " ")
        );
        self.view.notice_error = true;
        self.view.notice_until = None;
    }

    pub fn tick(&mut self) {
        #[cfg(unix)]
        self.tick_remote();
        #[cfg(target_os = "linux")]
        self.tick_mpris_commands();
        if let Err(error) = self.tick_inner() {
            self.failure(error);
        }
        // Save even when another background operation failed during this tick.
        if let Err(error) = self.tick_session() {
            self.failure(error);
        }
        #[cfg(target_os = "linux")]
        self.publish_mpris();
    }

    fn tick_inner(&mut self) -> Result<()> {
        self.tick_lyrics();
        self.tick_downloads()?;
        self.tick_services()?;
        self.tick_mods()?;
        self.tick_tags();
        self.tick_statistics()?;
        self.tick_database()?;
        self.tick_settings()?;
        self.tick_settings_files()?;
        self.tick_browser()?;
        self.tick_playback()?;
        self.tick_seek()?;
        self.tick_artwork();
        self.tick_waveform();
        self.poll_import()?;
        self.tick_library()?;
        self.tick_mark_drag()?;
        self.tick_search()?;
        self.tick_visualizer();
        if !self.preparing_playback()
            && self.playback.seek_preview.is_none()
            && self.playback.audio.as_mut().is_some_and(Audio::finished)
        {
            self.finish_listening();
            if let Err(error) = self.next(1, false) {
                self.playback.current = None;
                return Err(error);
            }
        }
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Capture the user's play/pause state before shutdown pauses the device.
        let session = self.session_snapshot();
        #[cfg(target_os = "linux")]
        self.mpris.take();
        self.cancel_seek();
        if let Some(audio) = &self.playback.audio {
            audio.set_paused(true);
        }
        self.finish_listening();
        self.downloads.cancel_all();
        self.services.cancel_all();
        self.mods.cancel_all();
        self.cancel_audio_preparation();
        self.cancel_waveform();
        if let Some(job) = &self.library.import {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let file_edit_finished = match self.shutdown_file_edit() {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "Almavorn: file write was not completed; pending settings were not written / запись файла не завершена; отложенные настройки не записаны: {error:#}"
                );
                false
            }
        };
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        // Only shutdown waits for persistence. Share a deadline with the worker
        // shutdown because started blocking calls cannot be aborted.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        if let Err(error) =
            session.and_then(|snapshot| self.shutdown_session(&runtime, deadline, snapshot))
        {
            eprintln!("Almavorn: playback session was not saved / сеанс прослушивания не сохранен: {error:#}");
        }
        if let Err(error) = self.shutdown_statistics(&runtime, deadline) {
            eprintln!(
                "Almavorn: listening history was not saved / статистика не сохранена: {error:#}"
            );
        }
        if file_edit_finished && let Err(error) = self.shutdown_settings(&runtime, deadline) {
            eprintln!("Almavorn: settings were not saved / настройки не сохранены: {error:#}");
        }
        runtime.shutdown_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
    }
}

fn bounded(position: usize, direction: i64, length: usize) -> usize {
    (position as i128 + direction as i128).clamp(0, length.saturating_sub(1) as i128) as usize
}
