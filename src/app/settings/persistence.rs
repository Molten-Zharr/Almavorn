use super::App;
use crate::app::background::{Background, background, poll};
use anyhow::{Context, Result};
use std::time::Instant;
use tokio::runtime::Runtime;

impl App {
    /// Queue the latest settings snapshot without changing playback.
    pub(in crate::app) fn save_settings(&mut self) -> Result<()> {
        self.settings.sync_active_profile();
        self.persistence.pending = Some(self.settings.clone());
        self.persistence.failed = false;
        self.start_settings()
    }

    pub fn settings_saving(&self) -> bool {
        self.persistence.job.is_some() || self.persistence.pending.is_some()
    }

    fn start_settings(&mut self) -> Result<()> {
        if !self.persistence.failed
            && self.persistence.job.is_none()
            && let Some(settings) = &self.persistence.pending
        {
            let mut store = self.store.try_clone()?;
            let settings = settings.clone();
            self.persistence.job = Some(background(self.runtime(), move || {
                store.save_settings(&settings)
            }));
            self.persistence.pending = None;
        }
        Ok(())
    }

    pub(in crate::app) fn tick_settings(&mut self) -> Result<()> {
        let result = self.persistence.job.as_mut().and_then(poll);
        if let Some(result) = result {
            self.persistence.job = None;
            if let Err(error) = result {
                self.persistence
                    .pending
                    .get_or_insert_with(|| self.settings.clone());
                self.persistence.failed = true;
                return Err(error);
            }
        }
        self.start_settings()
    }

    pub(in crate::app) fn shutdown_settings(
        &mut self,
        runtime: &Runtime,
        deadline: Instant,
    ) -> Result<()> {
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
        if let Some(job) = self.persistence.job.take() {
            // A timeout leaves the blocking writer running, so do not start a
            // second write that could race it and restore an older snapshot.
            result = wait(job)?;
            if result.is_err() {
                self.persistence
                    .pending
                    .get_or_insert_with(|| self.settings.clone());
            }
        }
        if let Some(settings) = self.persistence.pending.take() {
            let mut store = self.store.try_clone()?;
            result = wait(background(runtime, move || store.save_settings(&settings)))?;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::oneshot;

    fn finish_saving(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.settings_saving() {
            assert!(Instant::now() < deadline, "Settings save timed out");
            app.tick_settings().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn changes_during_a_write_coalesce_into_the_latest_profile_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        let original = serde_json::to_value(app.store.settings().unwrap()).unwrap();
        let (sender, receiver) = oneshot::channel();
        app.persistence.job = Some(receiver);

        app.set_volume(0.25).unwrap();
        app.settings.preferences.appearance.font_size = 20;
        app.save_settings().unwrap();
        app.set_volume(0.65).unwrap();
        app.settings.preferences.appearance.font_size = 24;
        app.save_settings().unwrap();
        app.tick_settings().unwrap();
        assert!(app.persistence.job.is_some());
        assert_eq!(
            serde_json::to_value(app.store.settings().unwrap()).unwrap(),
            original,
            "A running write must finish before the next snapshot starts"
        );

        let expected = serde_json::to_value(&app.settings).unwrap();
        sender.send(Ok(())).unwrap();
        finish_saving(&mut app);
        drop(app);
        let app = App::new(directory.path()).unwrap();
        assert_eq!(serde_json::to_value(&app.settings).unwrap(), expected);
        assert_eq!(app.settings.profiles[0].preferences.volume, 0.65);
        assert_eq!(
            app.settings.profiles[0].preferences.appearance.font_size,
            24
        );
    }

    #[test]
    fn failed_write_retains_the_latest_settings_and_retries_after_a_change() {
        for newer_snapshot in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new(directory.path()).unwrap();
            let (sender, receiver) = oneshot::channel();
            app.persistence.job = Some(receiver);
            if newer_snapshot {
                app.set_volume(0.25).unwrap();
            }
            let expected = serde_json::to_value(&app.settings).unwrap();
            sender
                .send(Err(anyhow::anyhow!("Simulated write failure")))
                .unwrap();
            assert!(app.tick_settings().is_err());
            assert_eq!(
                serde_json::to_value(app.persistence.pending.as_ref().unwrap()).unwrap(),
                expected
            );
            app.tick_settings().unwrap();
            assert!(app.persistence.failed);
            assert!(app.persistence.job.is_none());
            assert!(app.settings_saving());

            app.set_volume(0.45).unwrap();
            finish_saving(&mut app);
            let saved = app.store.settings().unwrap();
            assert_eq!(saved.preferences.volume, 0.45);
            assert_eq!(saved.profiles[0].preferences.volume, 0.45);
        }
    }

    #[test]
    fn shutdown_flushes_the_latest_snapshot_even_after_a_failed_write_without_a_tick() {
        for failed_write in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut app = App::new(directory.path()).unwrap();
            let (sender, receiver) = oneshot::channel();
            app.persistence.job = Some(receiver);
            app.set_volume(0.25).unwrap();
            app.set_volume(0.65).unwrap();
            let expected = serde_json::to_value(&app.settings).unwrap();
            sender
                .send(if failed_write {
                    Err(anyhow::anyhow!("Simulated write failure"))
                } else {
                    Ok(())
                })
                .unwrap();
            drop(app);
            let app = App::new(directory.path()).unwrap();
            assert_eq!(serde_json::to_value(&app.settings).unwrap(), expected);
        }
    }

    #[test]
    fn shutdown_retries_a_failed_write_when_there_is_no_newer_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        app.settings.preferences.volume = 0.35;
        app.settings.sync_active_profile();
        let expected = serde_json::to_value(&app.settings).unwrap();
        let (sender, receiver) = oneshot::channel();
        app.persistence.job = Some(receiver);
        sender
            .send(Err(anyhow::anyhow!("Simulated write failure")))
            .unwrap();
        drop(app);
        let app = App::new(directory.path()).unwrap();
        assert_eq!(serde_json::to_value(&app.settings).unwrap(), expected);
    }
}
