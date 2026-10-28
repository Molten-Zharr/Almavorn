use super::{App, Target, background::background};
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

impl App {
    pub fn file_drag_failed(&mut self, error: impl std::fmt::Display) {
        self.view.notice = format!(
            "{}: {error}",
            self.text("Cannot drag files", "Не удалось перетащить файлы")
        );
        self.view.notice_error = true;
        self.view.notice_until = None;
    }

    pub(super) fn open_file_manager(&mut self, path: PathBuf) {
        self.file_manager.request(path);
    }

    pub(super) fn tick_file_manager(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        let language = self.settings.preferences.language;
        self.file_manager.start(|path| {
            let path = path.clone();
            let handle = runtime.handle().clone();
            background(runtime, move || {
                ensure!(
                    path.is_file(),
                    "{}: {}",
                    language.text("Audio file not found", "Аудиофайл не найден"),
                    path.display()
                );
                let path = path.canonicalize()?;
                if handle.block_on(reveal(&path)).is_ok() {
                    return Ok(None);
                }
                // File managers without FileManager1 still open the containing folder.
                let folder = path.parent().context("File has no parent folder")?;
                let child = Command::new("xdg-open")
                    .arg(folder)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .context(language.text(
                        "Cannot start the file manager",
                        "Не удалось запустить проводник",
                    ))?;
                Ok(Some(child))
            })
        });
        if let Some(done) = self.file_manager.poll() {
            match done.result {
                Ok(None) if done.current => self.message(
                    self.text("File shown in file manager", "Файл показан в проводнике")
                        .into(),
                ),
                Ok(Some(child)) => {
                    self.file_manager_processes.push(child);
                    if done.current {
                        self.message(
                            self.text("Opening containing folder", "Открывается папка с файлом")
                                .into(),
                        );
                    }
                }
                Err(error) if done.current => self.failure(error),
                _ => {}
            }
        }
        let mut failed = false;
        self.file_manager_processes
            .retain_mut(|child| match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) if status.success() => false,
                _ => {
                    failed = true;
                    false
                }
            });
        if failed {
            self.failure(anyhow::anyhow!(self.text(
                "Cannot open the folder. Check your default file manager.",
                "Не удалось открыть папку. Проверьте проводник по умолчанию."
            )));
        }
    }

    /// Snapshot the files at the press location, before hover or selection can change.
    pub fn drag_file_paths_at(&self, x: u16, y: u16) -> Vec<PathBuf> {
        if self.view.dialog.is_some() || self.view.layout_editing {
            return Vec::new();
        }
        let paths = match self.target_at((x, y).into()) {
            Some(Target::PlayingTrack) => self
                .playback
                .current
                .iter()
                .map(|track| track.path.clone())
                .collect(),
            Some(Target::Track(id)) => self
                .playlist()
                .into_iter()
                .flat_map(|playlist| &playlist.entries)
                .filter(|entry| {
                    entry.id == *id
                        || (self.library.marked.contains(id)
                            && self.library.marked.contains(&entry.id))
                })
                .map(|entry| entry.track.path.clone())
                .collect(),
            _ => Vec::new(),
        };
        // Service URLs must never be exported as local paths or downloaded implicitly.
        if paths.iter().any(|path: &PathBuf| !path.is_absolute()) {
            Vec::new()
        } else {
            paths
        }
    }
}

#[cfg(target_os = "linux")]
async fn reveal(path: &Path) -> Result<()> {
    let uri = url::Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid file path"))?;
    let connection = zbus::connection::Builder::session()?
        .method_timeout(Duration::from_secs(5))
        .build()
        .await?;
    connection
        .call_method(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            Some("org.freedesktop.FileManager1"),
            "ShowItems",
            &(vec![uri.as_str()], ""),
        )
        .await?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn reveal(_: &Path) -> Result<()> {
    anyhow::bail!("File manager integration is unavailable")
}
