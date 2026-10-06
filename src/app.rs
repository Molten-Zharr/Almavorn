mod actions;
mod background;
mod browser;
mod context;
mod database;
mod dialogs;
mod events;
mod history;
mod import;
mod library;
mod options;
mod playback;
mod settings;
mod state;
mod workspace;

pub use browser::{Browser, BrowserEntry};
use context::{BrowserState, SettingsPersistence};
pub use context::{LibraryState, PlaybackState, UiState};
pub use dialogs::{Dialog, TextDialog, TextPurpose};
pub use options::Options;
pub(crate) use settings::SettingControl;
pub use settings::{SettingsCatalog, SettingsEdit, SettingsFocus, SettingsPage};
pub use state::{Focus, Hit, Keycap, Sort, Target};

use self::background::{Background, background};
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
    pub view: UiState,
    browser_state: BrowserState,
    persistence: SettingsPersistence,
}

impl App {
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
        settings.volume = settings.volume.clamp(0.0, 1.0);
        settings.workspace.normalize();
        for binding in crate::input::default_bindings() {
            if !settings
                .bindings
                .iter()
                .any(|value| value.action == binding.action)
            {
                settings.bindings.push(binding);
            }
        }
        let library = LibraryState::new(store.playlists()?, settings.mode, store.revision());
        let view = UiState::new(settings.language);
        Ok(Self {
            store,
            runtime: Some(runtime),
            settings,
            library,
            view,
            playback: Default::default(),
            browser_state: Default::default(),
            persistence: Default::default(),
        })
    }

    pub fn text(&self, english: &'static str, russian: &'static str) -> &'static str {
        self.settings.language.text(english, russian)
    }

    fn message(&mut self, text: String) {
        self.view.notice = text;
        self.view.notice_error = false;
    }

    fn failure(&mut self, error: anyhow::Error) {
        let reason = crate::errors::message(&error, self.settings.language);
        self.view.notice = format!(
            "{}: {}",
            self.text("Action failed", "Действие не выполнено"),
            reason.replace('\n', " ")
        );
        self.view.notice_error = true;
    }

    pub fn tick(&mut self) {
        if let Err(error) = self.tick_inner() {
            self.failure(error);
        }
    }

    fn tick_inner(&mut self) -> Result<()> {
        self.tick_database()?;
        self.tick_settings()?;
        self.tick_settings_files()?;
        self.tick_browser()?;
        self.tick_playback()?;
        self.tick_seek()?;
        self.tick_waveform();
        self.poll_import()?;
        self.tick_library()?;
        if !self.preparing_playback()
            && self.playback.seek_preview.is_none()
            && self.playback.audio.as_mut().is_some_and(Audio::finished)
            && let Err(error) = self.next(1, false)
        {
            self.playback.current = None;
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.cancel_waveform();
        if let Some(job) = &self.library.import {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        // Only shutdown waits for settings. Share a deadline with the worker shutdown
        // because started blocking calls cannot be aborted, including settings writes.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let wait = |job: Background<()>| {
            runtime
                .block_on(async {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), job).await
                })
                .context("Settings save did not finish before shutdown")
                .map(|result| {
                    result
                        .context("Settings save was interrupted")
                        .and_then(|result| result)
                })
        };
        let mut result = Ok(());
        let mut expired = false;
        if let Some(job) = self.persistence.job.take() {
            match wait(job) {
                Ok(saved) => {
                    if saved.is_err() {
                        self.persistence
                            .pending
                            .get_or_insert_with(|| self.settings.clone());
                    }
                    result = saved;
                }
                Err(error) => {
                    expired = true;
                    result = Err(error);
                }
            }
        }
        if !expired && let Some(settings) = self.persistence.pending.take() {
            result = self.store.try_clone().and_then(|mut store| {
                let job = background(&runtime, move || store.save_settings(&settings));
                wait(job).and_then(|result| result)
            });
        }
        if let Err(error) = result {
            eprintln!("Almavorn: settings were not saved / настройки не сохранены: {error:#}");
        }
        runtime.shutdown_timeout(deadline.saturating_duration_since(std::time::Instant::now()));
    }
}

fn bounded(position: usize, direction: i64, length: usize) -> usize {
    (position as i128 + direction as i128).clamp(0, length.saturating_sub(1) as i128) as usize
}
