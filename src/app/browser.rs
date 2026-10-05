use super::{
    App, Dialog,
    database::{Background, background},
};
use crate::media;
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, path::PathBuf};
use tokio::sync::oneshot;

pub(super) struct BrowserJob {
    receiver: Background<Browser>,
    generation: u64,
}
pub(super) struct BrowserRequest {
    directory: PathBuf,
    folder: bool,
    generation: u64,
    initial: bool,
}

#[derive(Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub directory: bool,
}
#[derive(Clone)]
pub struct Browser {
    pub directory: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: usize,
    pub offset: usize,
    pub marked: HashSet<PathBuf>,
    pub folder: bool,
}

impl Browser {
    fn unloaded(directory: PathBuf, folder: bool) -> Self {
        Self {
            directory,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            marked: HashSet::new(),
            folder,
        }
    }
    pub(super) fn open(directory: PathBuf, folder: bool) -> Result<Self> {
        let mut browser = Self::unloaded(directory, folder);
        browser.refresh()?;
        Ok(browser)
    }
    pub(super) fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        for entry in std::fs::read_dir(&self.directory)
            .with_context(|| format!("Cannot read folder {}", self.directory.display()))?
            .collect::<std::io::Result<Vec<_>>>()?
        {
            let directory = entry.path().is_dir();
            if directory || (!self.folder && media::is_audio(&entry.path())) {
                self.entries.push(BrowserEntry {
                    path: entry.path(),
                    directory,
                });
            }
        }
        self.entries.sort_by_key(|entry| {
            (
                !entry.directory,
                entry.path.file_name().unwrap_or_default().to_os_string(),
            )
        });
        self.selected = 0;
        self.offset = 0;
        Ok(())
    }
}

impl App {
    pub(super) fn show_browser(&mut self, directory: PathBuf, folder: bool) {
        self.dialog = Some(Dialog::Browser(Browser::unloaded(
            directory.clone(),
            folder,
        )));
        self.queue_browser(directory, folder, true);
    }
    fn queue_browser(&mut self, directory: PathBuf, folder: bool, initial: bool) {
        self.browser_generation = self.browser_generation.wrapping_add(1);
        self.browser_ready = false;
        if let Some(Dialog::Browser(browser)) = &mut self.dialog {
            browser.directory = directory.clone();
            browser.entries.clear();
            browser.selected = 0;
            browser.offset = 0;
        }
        self.pending_browser = Some(BrowserRequest {
            directory,
            folder,
            initial,
            generation: self.browser_generation,
        });
        self.start_browser_job();
        self.message(self.text("Reading folder...", "Чтение папки...").into());
    }
    fn start_browser_job(&mut self) {
        if self.browser_job.is_none()
            && let Some(request) = self.pending_browser.take()
        {
            let generation = request.generation;
            self.browser_job = Some(BrowserJob {
                generation,
                receiver: background(self.runtime(), move || {
                    let directory = if request.initial && !request.directory.is_dir() {
                        dirs::home_dir().unwrap_or(request.directory)
                    } else {
                        request.directory
                    };
                    Browser::open(directory, request.folder)
                }),
            });
        }
    }
    pub fn browser_loading(&self) -> bool {
        self.pending_browser.is_some()
            || self
                .browser_job
                .as_ref()
                .is_some_and(|job| job.generation == self.browser_generation)
    }
    pub fn browser_ready(&self) -> bool {
        self.browser_ready
    }
    pub(super) fn cancel_browser(&mut self) {
        self.browser_generation = self.browser_generation.wrapping_add(1);
        self.pending_browser = None;
        self.browser_ready = false;
    }
    pub(super) fn tick_browser(&mut self) -> Result<()> {
        let result = match self.browser_job.as_mut().map(|job| job.receiver.try_recv()) {
            Some(Ok(result)) => result,
            Some(Err(oneshot::error::TryRecvError::Closed)) => {
                Err(anyhow::anyhow!("Background operation was interrupted"))
            }
            _ => return Ok(()),
        };
        let job = self.browser_job.take().expect("folder read is present");
        self.start_browser_job();
        if job.generation == self.browser_generation
            && let Some(Dialog::Browser(browser)) = &mut self.dialog
        {
            let loaded = result?;
            browser.directory = loaded.directory;
            browser.entries = loaded.entries;
            self.browser_ready = true;
            self.message(self.text("Folder loaded", "Папка прочитана").into());
        }
        Ok(())
    }
    pub(super) fn browser_parent(&mut self) {
        if let Some(Dialog::Browser(browser)) = &self.dialog
            && let Some(parent) = browser.directory.parent()
        {
            self.queue_browser(parent.to_owned(), browser.folder, false);
        }
    }
    pub(super) fn browser_open(&mut self) -> Result<()> {
        let directory = if let Some(Dialog::Browser(browser)) = &mut self.dialog
            && let Some(entry) = browser.entries.get(browser.selected)
        {
            if entry.directory {
                Some((entry.path.clone(), browser.folder))
            } else {
                if !browser.marked.insert(entry.path.clone()) {
                    browser.marked.remove(&entry.path);
                }
                None
            }
        } else {
            None
        };
        if let Some((directory, folder)) = directory {
            self.queue_browser(directory, folder, false);
        }
        Ok(())
    }

    pub(super) fn browser_add(&mut self) -> Result<()> {
        ensure!(
            self.browser_ready,
            "Choose an available folder before adding music"
        );
        let paths = if let Some(Dialog::Browser(browser)) = &self.dialog {
            if browser.folder {
                vec![browser.directory.clone()]
            } else if !browser.marked.is_empty() {
                let mut paths: Vec<_> = browser.marked.iter().cloned().collect();
                paths.sort();
                paths
            } else {
                browser
                    .entries
                    .get(browser.selected)
                    .filter(|entry| !entry.directory)
                    .map(|entry| vec![entry.path.clone()])
                    .unwrap_or_default()
            }
        } else {
            return Ok(());
        };
        ensure!(!paths.is_empty(), "Select music files first");
        self.start_import(paths)?;
        self.cancel_browser();
        self.dialog = None;
        Ok(())
    }
}
