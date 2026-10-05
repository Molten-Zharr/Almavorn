use super::{App, Dialog};
use crate::media;
use anyhow::{Result, ensure};
use std::{collections::HashSet, path::PathBuf};

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
    pub(super) fn open(directory: PathBuf, folder: bool) -> Result<Self> {
        let mut browser = Self {
            directory,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            marked: HashSet::new(),
            folder,
        };
        browser.refresh()?;
        Ok(browser)
    }
    pub(super) fn refresh(&mut self) -> Result<()> {
        self.entries.clear();
        for entry in std::fs::read_dir(&self.directory)?.collect::<std::io::Result<Vec<_>>>()? {
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
    pub(super) fn browser_open(&mut self) -> Result<()> {
        if let Some(Dialog::Browser(browser)) = &mut self.dialog
            && let Some(entry) = browser.entries.get(browser.selected)
        {
            if entry.directory {
                browser.directory = entry.path.clone();
                browser.refresh()?;
            } else {
                if !browser.marked.insert(entry.path.clone()) {
                    browser.marked.remove(&entry.path);
                }
            }
        }
        Ok(())
    }

    pub(super) fn browser_add(&mut self) -> Result<()> {
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
        self.dialog = None;
        Ok(())
    }
}
