use super::{App, Dialog, background::background};
use crate::errors::AppError;
use crate::media;
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, path::PathBuf};

pub(super) struct BrowserRequest {
    directory: PathBuf,
    folder: bool,
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
        self.view.dialog = Some(Dialog::Browser(Browser::unloaded(
            directory.clone(),
            folder,
        )));
        self.queue_browser(directory, folder, true);
    }
    fn queue_browser(&mut self, directory: PathBuf, folder: bool, initial: bool) {
        self.browser_state.ready = false;
        if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
            browser.directory = directory.clone();
            browser.entries.clear();
            browser.selected = 0;
            browser.offset = 0;
        }
        self.browser_state.job.request(BrowserRequest {
            directory,
            folder,
            initial,
        });
        self.start_browser_job();
        self.message(self.text("Reading folder...", "Чтение папки...").into());
    }
    fn start_browser_job(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.browser_state.job.start(|request| {
            let BrowserRequest {
                directory,
                folder,
                initial,
            } = request;
            let (directory, folder, initial) = (directory.clone(), *folder, *initial);
            background(runtime, move || {
                let directory = if initial && !directory.is_dir() {
                    dirs::home_dir().unwrap_or(directory)
                } else {
                    directory
                };
                Browser::open(directory, folder)
            })
        });
    }
    pub fn browser_loading(&self) -> bool {
        self.browser_state.job.requested().is_some()
    }
    pub fn browser_ready(&self) -> bool {
        self.browser_state.ready
    }
    pub(super) fn cancel_browser(&mut self) {
        self.browser_state.job.cancel();
        self.browser_state.ready = false;
    }
    pub(super) fn tick_browser(&mut self) -> Result<()> {
        let Some(completed) = self.browser_state.job.poll() else {
            return Ok(());
        };
        self.start_browser_job();
        if completed.current
            && let Some(Dialog::Browser(browser)) = &mut self.view.dialog
        {
            let loaded = completed.result?;
            browser.directory = loaded.directory;
            browser.entries = loaded.entries;
            self.browser_state.ready = true;
            self.message(self.text("Folder loaded", "Папка прочитана").into());
        }
        Ok(())
    }
    pub(super) fn browser_parent(&mut self) {
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(parent) = browser.directory.parent()
        {
            self.queue_browser(parent.to_owned(), browser.folder, false);
        }
    }
    pub(super) fn browser_open(&mut self) -> Result<()> {
        let directory = if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
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
        ensure!(self.browser_state.ready, AppError::FolderNotReady);
        let paths = if let Some(Dialog::Browser(browser)) = &self.view.dialog {
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
        ensure!(!paths.is_empty(), AppError::MusicSelectionRequired);
        self.start_import(paths)?;
        self.cancel_browser();
        self.view.dialog = None;
        Ok(())
    }
}
