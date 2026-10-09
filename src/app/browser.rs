use super::{App, Dialog, TextDialog, TextPurpose, background::background};
use crate::errors::AppError;
use crate::media;
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub(super) struct BrowserRequest {
    directory: PathBuf,
    folder: bool,
    initial: bool,
    focus: Option<PathBuf>,
    offset: usize,
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
    pub selection_explicit: bool,
    pub marked: HashSet<PathBuf>,
    pub folder: bool,
    pub playlist_name: Option<TextDialog>,
    offsets: HashMap<PathBuf, usize>,
}

impl Browser {
    fn unloaded(directory: PathBuf, folder: bool) -> Self {
        Self {
            directory,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            selection_explicit: false,
            marked: HashSet::new(),
            folder,
            playlist_name: None,
            offsets: HashMap::new(),
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
        self.selection_explicit = false;
        Ok(())
    }

    pub fn chosen_folder(&self) -> &Path {
        if self.selection_explicit
            && let Some(entry) = self.entries.get(self.selected)
            && entry.directory
        {
            &entry.path
        } else {
            &self.directory
        }
    }
}

impl App {
    pub(super) fn choose_playlist_name_folder(&mut self) -> Result<()> {
        ensure!(!self.busy(), AppError::LibraryBusy);
        let Some(Dialog::Text(dialog)) = &self.view.dialog else {
            return Ok(());
        };
        if !matches!(
            dialog.purpose,
            TextPurpose::Create | TextPurpose::CreateFromFolder { .. }
        ) {
            return Ok(());
        }
        let dialog = dialog.clone();
        let directory = if let TextPurpose::CreateFromFolder { folder, .. } = &dialog.purpose {
            folder.clone()
        } else {
            dirs::audio_dir()
                .or_else(dirs::home_dir)
                .unwrap_or(std::env::current_dir()?)
        };
        self.show_browser(directory, true);
        if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
            browser.playlist_name = Some(dialog);
        }
        Ok(())
    }

    pub(super) fn show_browser(&mut self, directory: PathBuf, folder: bool) {
        self.view.dialog = Some(Dialog::Browser(Browser::unloaded(
            directory.clone(),
            folder,
        )));
        self.queue_browser(directory, folder, true, None);
    }
    fn queue_browser(
        &mut self,
        directory: PathBuf,
        folder: bool,
        initial: bool,
        focus: Option<PathBuf>,
    ) {
        let ready = self.browser_state.ready;
        self.browser_state.ready = false;
        let mut offset = 0;
        if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
            if ready {
                browser
                    .offsets
                    .insert(browser.directory.clone(), browser.offset);
            }
            if focus.is_some() {
                offset = browser.offsets.get(&directory).copied().unwrap_or(0);
            }
            browser.directory = directory.clone();
            browser.entries.clear();
            browser.selected = 0;
            browser.offset = 0;
            browser.selection_explicit = false;
        }
        self.browser_state.job.request(BrowserRequest {
            directory,
            folder,
            initial,
            focus,
            offset,
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
                focus,
                offset,
            } = request;
            let (directory, folder, initial) = (directory.clone(), *folder, *initial);
            let (focus, offset) = (focus.clone(), *offset);
            background(runtime, move || {
                let directory = if initial && !directory.is_dir() {
                    dirs::home_dir().unwrap_or(directory)
                } else {
                    directory
                };
                let mut browser = Browser::open(directory, folder)?;
                if let Some(focus) = focus
                    && let Some(index) =
                        browser.entries.iter().position(|entry| entry.path == focus)
                {
                    browser.selected = index;
                    browser.offset = offset;
                    browser.selection_explicit = true;
                }
                Ok(browser)
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
            browser.selected = loaded.selected;
            browser.offset = loaded.offset;
            browser.selection_explicit = loaded.selection_explicit;
            self.browser_state.ready = true;
            self.message(self.text("Folder loaded", "Папка прочитана").into());
        }
        Ok(())
    }
    pub(super) fn browser_parent(&mut self) {
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(parent) = browser.directory.parent()
        {
            self.queue_browser(
                parent.to_owned(),
                browser.folder,
                false,
                Some(browser.directory.clone()),
            );
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
            self.queue_browser(directory, folder, false, None);
        }
        Ok(())
    }

    pub(super) fn browser_add(&mut self) -> Result<()> {
        ensure!(self.browser_state.ready, AppError::FolderNotReady);
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(dialog) = &browser.playlist_name
        {
            let name = browser
                .chosen_folder()
                .file_name()
                .and_then(|name| name.to_str())
                .context(self.text("Choose a folder with a name", "Выберите папку с названием"))?;
            let name: String = name
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            ensure!(!name.trim().is_empty(), AppError::InvalidTrackName);
            let name = if matches!(
                name.trim().to_lowercase().as_str(),
                "sorting desk" | "сортировочный стол"
            ) {
                format!("{} (1)", name.trim())
            } else {
                name.trim().to_owned()
            };
            let mut dialog = dialog.clone();
            dialog.text = self.fresh_playlist_name(&name);
            let group_subfolders = matches!(
                dialog.purpose,
                TextPurpose::CreateFromFolder {
                    group_subfolders: true,
                    ..
                }
            );
            dialog.purpose = TextPurpose::CreateFromFolder {
                folder: browser.chosen_folder().to_owned(),
                group_subfolders,
            };
            dialog.selected_all = true;
            self.cancel_browser();
            self.view.dialog = Some(Dialog::Text(dialog));
            return Ok(());
        }
        let paths = if let Some(Dialog::Browser(browser)) = &self.view.dialog {
            if browser.folder {
                vec![browser.chosen_folder().to_owned()]
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
