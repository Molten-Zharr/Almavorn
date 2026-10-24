use super::{App, Dialog, SettingsEdit, Target, TextDialog, TextPurpose, background::background};
use crate::errors::AppError;
use crate::input::{Key, KeyPress};
use crate::media;
use anyhow::{Context, Result, ensure};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub(super) struct BrowserRequest {
    directory: PathBuf,
    folder: bool,
    palette: bool,
    initial: bool,
    focus: Option<PathBuf>,
    offset: usize,
}

#[derive(Clone)]
pub enum PaletteFilePicker {
    Import,
    Export { id: String, filename: String },
}

#[derive(Clone)]
pub struct BrowserEntry {
    pub path: PathBuf,
    pub directory: bool,
}
#[derive(Clone,Copy,Default,PartialEq,Eq)]
pub enum BrowserSort{#[default]Name,Modified,Size,Type}
impl BrowserSort{
    pub fn name(self,language:crate::model::Language)->&'static str{match self{Self::Name=>language.text("Name","Имя"),Self::Modified=>language.text("Modified","Изменение"),Self::Size=>language.text("File size","Размер файла"),Self::Type=>language.text("File type","Тип файла")}}
    fn next(self)->Self{match self{Self::Name=>Self::Modified,Self::Modified=>Self::Size,Self::Size=>Self::Type,Self::Type=>Self::Name}}
}

#[derive(Clone)]
pub struct Browser {
    all_entries: Vec<BrowserEntry>,
    details: HashMap<PathBuf,(Option<std::time::SystemTime>,u64)>,
    pub sort: BrowserSort,
    pub descending: bool,
    pub directory: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: usize,
    pub offset: usize,
    pub selection_explicit: bool,
    pub marked: HashSet<PathBuf>,
    pub search: String,
    pub folder: bool,
    /// Some(group_subfolders) selects one playlist per checked directory.
    pub folder_playlists: Option<bool>,
    pub playlist_name: Option<TextDialog>,
    pub palette_file: Option<PaletteFilePicker>,
    offsets: HashMap<PathBuf, usize>,
}

impl Browser {
    fn unloaded(directory: PathBuf, folder: bool) -> Self {
        Self {
            directory,
            all_entries:Vec::new(),details:HashMap::new(),sort:BrowserSort::Name,descending:false,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            selection_explicit: false,
            marked: HashSet::new(),
            search: String::new(),
            folder,
            folder_playlists: None,
            playlist_name: None,
            palette_file: None,
            offsets: HashMap::new(),
        }
    }
    fn open(directory: PathBuf, folder: bool, palette: bool) -> Result<Self> {
        let mut browser = Self::unloaded(directory, folder);
        browser.refresh(palette)?;
        Ok(browser)
    }
    fn refresh(&mut self, palette: bool) -> Result<()> {
        self.entries.clear();
        self.details.clear();
        for entry in std::fs::read_dir(&self.directory)
            .with_context(|| format!("Cannot read folder {}", self.directory.display()))?
            .collect::<std::io::Result<Vec<_>>>()?
        {
            let directory = entry.path().is_dir();
            let selectable = if palette {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            } else {
                media::is_audio(&entry.path())
            };
            if directory || (!self.folder && selectable) {
                if let Ok(metadata)=std::fs::metadata(entry.path()){self.details.insert(entry.path(),(metadata.modified().ok(),metadata.len()));}
                self.entries.push(BrowserEntry {
                    path: entry.path(),
                    directory,
                });
            }
        }
        self.all_entries=self.entries.clone();
        self.rebuild_entries();
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

    pub fn can_mark(&self, entry: &BrowserEntry) -> bool {
        self.palette_file.is_none()
            && if self.folder_playlists.is_some() {
                entry.directory
            } else {
                !entry.directory
            }
    }

    pub fn search_failed(&self)->bool{!self.search.is_empty()&&self.entries.is_empty()}
    fn select_search_match(&mut self){self.rebuild_entries();self.selection_explicit=!self.search.is_empty()&&!self.entries.is_empty();}
    fn rebuild_entries(&mut self){
        let selected=self.entries.get(self.selected).map(|entry|entry.path.clone());
        let details=&self.details;let sort=self.sort;let descending=self.descending;
        self.all_entries.sort_by(|a,b|{
            let name=||a.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase().cmp(&b.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase());
            let order=match sort{
                BrowserSort::Name=>name(),
                BrowserSort::Modified=>details.get(&a.path).and_then(|d|d.0).cmp(&details.get(&b.path).and_then(|d|d.0)),
                BrowserSort::Size if !a.directory&&!b.directory=>details.get(&a.path).map(|d|d.1).cmp(&details.get(&b.path).map(|d|d.1)),
                BrowserSort::Type if !a.directory&&!b.directory=>a.path.extension().unwrap_or_default().to_string_lossy().to_lowercase().cmp(&b.path.extension().unwrap_or_default().to_string_lossy().to_lowercase()),
                _=>name(),
            };
            (!a.directory).cmp(&(!b.directory)).then_with(||if descending{order.reverse()}else{order}).then_with(name).then_with(||a.path.cmp(&b.path))
        });
        let query=self.search.to_lowercase();self.entries=self.all_entries.iter().filter(|entry|entry.path.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(&query)).cloned().collect();
        self.selected=selected.and_then(|path|self.entries.iter().position(|entry|entry.path==path)).unwrap_or(0);self.offset=0;
    }
    pub(super) fn clear_filter(&mut self){self.search.clear();self.rebuild_entries();}
    pub(super) fn change_sort(&mut self,reverse:bool){if reverse{self.descending=!self.descending;}else{self.sort=self.sort.next();self.descending=matches!(self.sort,BrowserSort::Modified|BrowserSort::Size);}self.rebuild_entries();}

    pub(super) fn toggle_mark(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index)
            && self.can_mark(entry)
            && !self.marked.insert(entry.path.clone())
        {
            self.marked.remove(&entry.path);
        }
    }
}

impl App {
    pub(super) fn browser_text(&mut self, text: &str) -> Result<()> {
        let Some(Dialog::Browser(browser)) = &mut self.view.dialog else {
            return Ok(());
        };
        if text == " " && browser.search.is_empty() {
            if browser.folder
                && browser.folder_playlists.is_none()
                && browser.playlist_name.is_none()
                && browser.palette_file.is_none()
            {
                return self.target(Target::ScanSubfolders, false);
            }
            browser.toggle_mark(browser.selected);
        } else {
            let remaining = 256usize.saturating_sub(browser.search.chars().count());
            browser
                .search
                .extend(text.chars().filter(|c| !c.is_control()).take(remaining));
            browser.select_search_match();
        }
        self.view.pointer = ratatui::layout::Position::new(u16::MAX, u16::MAX);
        Ok(())
    }

    pub(super) fn browser_key(&mut self, key: KeyPress) -> Result<()> {
        if key.key == Key::Escape {
            if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                && !browser.search.is_empty()
            {
                browser.clear_filter();
            } else {
                self.close_dialog();
            }
            return Ok(());
        }
        if key.ctrl&&!key.alt&&matches!(key.key,Key::Char('s'|'S')){return self.target(if key.shift{Target::BrowserSortDirection}else{Target::BrowserSort},false);}
        if key.alt && !key.ctrl && key.key == Key::Up {
            return self.target(Target::BrowserParent, false);
        }
        if key.ctrl && !key.alt && key.key == Key::Char(' ') {
            if matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.folder && browser.folder_playlists.is_none() && browser.playlist_name.is_none() && browser.palette_file.is_none())
            {
                return self.target(Target::ScanSubfolders, false);
            }
            if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                && !browser.search_failed()
            {
                browser.toggle_mark(browser.selected);
            }
            return Ok(());
        }
        if key.ctrl
            && key.key == Key::Char('g')
            && matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.folder_playlists.is_some())
        {
            return self.target(Target::CreateFolderGroups, false);
        }
        if key.key == Key::Char('a') && key.ctrl {
            return self.target(Target::BrowserMarkAll, false);
        }
        if key.key == Key::Char('d') && key.ctrl {
            return self.target(Target::BrowserClearMarks, false);
        }
        if key.key == Key::Enter && key.ctrl {
            if key.key == Key::Enter
                && key.shift
                && matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.folder && browser.folder_playlists.is_none() && browser.palette_file.is_none())
            {
                return self.browser_add_current_folder();
            }
            return self.browser_add();
        }
        if key.ctrl || key.alt {
            return Ok(());
        }
        match key.key {
            Key::Char(character) => return self.browser_text(&character.to_string()),
            Key::Backspace => {
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                    && !browser.search.is_empty()
                {
                    browser.search.pop();
                    browser.select_search_match();
                    return Ok(());
                }
                return self.target(Target::BrowserParent, false);
            }
            Key::Home | Key::End => {
                self.scroll_dialog(if key.key == Key::Home {
                    i16::MIN
                } else {
                    i16::MAX
                });
                return Ok(());
            }
            Key::Insert => {
                if let Some(Dialog::Browser(browser)) = &mut self.view.dialog
                    && !browser.search_failed()
                {
                    browser.toggle_mark(browser.selected);
                }
                return Ok(());
            }
            _ => {}
        }
        self.dialog_navigation_key(key)
    }

    pub(super) fn show_folder_playlists(&mut self, directory: PathBuf) {
        self.show_browser(directory, true);
        if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
            browser.folder_playlists = Some(false);
        }
    }

    pub(super) fn restore_import_dialog(&mut self, dialog: Dialog) {
        self.view.dialog = Some(dialog);
        if let Some(Dialog::Browser(browser)) = &mut self.view.dialog {
            let directory = browser.directory.clone();
            let folder = browser.folder;
            let focus = browser
                .entries
                .get(browser.selected)
                .map(|entry| entry.path.clone());
            browser.offsets.insert(directory.clone(), browser.offset);
            self.queue_browser(directory, folder, false, focus);
        }
    }
    pub(super) fn choose_palette_import(&mut self) {
        self.show_palette_browser(PaletteFilePicker::Import, None);
    }

    pub(super) fn choose_palette_export_folder(&mut self) {
        let Some(Dialog::Text(dialog)) = &self.view.dialog else {
            return;
        };
        let TextPurpose::Settings(SettingsEdit::ExportPalette { id, directory }) = &dialog.purpose
        else {
            return;
        };
        self.show_palette_browser(
            PaletteFilePicker::Export {
                id: id.clone(),
                filename: dialog.text.clone(),
            },
            Some(directory.clone()),
        );
    }

    pub(super) fn show_palette_browser(
        &mut self,
        purpose: PaletteFilePicker,
        directory: Option<PathBuf>,
    ) {
        let directory = directory
            .or_else(|| self.view.settings.file_directory.clone())
            .or_else(dirs::document_dir)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| {
                self.store
                    .path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_owned()
            });
        let folder = matches!(purpose, PaletteFilePicker::Export { .. });
        self.view.dialog = Some(Dialog::Browser(Browser {
            palette_file: Some(purpose),
            ..Browser::unloaded(directory.clone(), folder)
        }));
        self.queue_browser(directory, folder, true, None);
    }

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
            browser.all_entries.clear();browser.details.clear();
            browser.selected = 0;
            browser.offset = 0;
            browser.selection_explicit = false;
            browser.search.clear();
        }
        self.browser_state.job.request(BrowserRequest {
            directory,
            folder,
            palette: matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.palette_file.is_some()),
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
                palette,
                initial,
                focus,
                offset,
            } = request;
            let (directory, folder, palette, initial) =
                (directory.clone(), *folder, *palette, *initial);
            let (focus, offset) = (focus.clone(), *offset);
            background(runtime, move || {
                let directory = if initial && !directory.is_dir() {
                    dirs::home_dir().unwrap_or(directory)
                } else {
                    directory
                };
                let mut browser = Browser::open(directory, folder, palette)?;
                if let Some(focus) = focus
                    && let Some(index) =
                        browser.entries.iter().position(|entry| entry.path == focus)
                {
                    browser.selected = index;
                    browser.offset = offset;
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
            browser.all_entries=loaded.all_entries;browser.details=loaded.details;
            browser.selected = loaded.selected;
            browser.offset = loaded.offset;
            // Restoring navigation focus does not confirm a choice of that child folder.
            browser.selection_explicit = false;
            browser.select_search_match();
            if browser.search.is_empty(){browser.offset=loaded.offset;}
            self.browser_state.ready = true;
            if !self.view.notice_error {
                self.message(self.text("Folder loaded", "Папка прочитана").into());
            }
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
    pub(super) fn browser_location(&mut self, documents: bool) {
        let directory = if documents {
            dirs::document_dir()
        } else {
            dirs::home_dir()
        };
        if let Some(directory) = directory
            && let Some(Dialog::Browser(browser)) = &self.view.dialog
        {
            self.queue_browser(directory, browser.folder, true, None);
        }
    }
    pub(super) fn browser_open(&mut self) -> Result<()> {
        if matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.search_failed()) {
            return Ok(());
        }
        if matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.palette_file.is_some()
            && browser.entries.get(browser.selected).is_some_and(|entry| !entry.directory))
        {
            return self.browser_add();
        }
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
        self.confirm_browser(false)
    }

    pub(super) fn browser_add_current_folder(&mut self) -> Result<()> {
        self.confirm_browser(true)
    }

    fn confirm_browser(&mut self, current_folder: bool) -> Result<()> {
        ensure!(self.browser_state.ready, AppError::FolderNotReady);
        if !current_folder
            && matches!(&self.view.dialog, Some(Dialog::Browser(browser)) if browser.search_failed() && browser.marked.is_empty())
        {
            return Ok(());
        }
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(group_subfolders) = browser.folder_playlists
        {
            let mut folders: Vec<_> = browser.marked.iter().cloned().collect();
            folders.sort();
            ensure!(!folders.is_empty(), AppError::FolderSelectionRequired);
            let recovery = self.view.dialog.clone();
            self.start_folder_playlists(folders, group_subfolders, recovery)?;
            self.cancel_browser();
            self.view.dialog = None;
            return Ok(());
        }
        let folder = if let Some(Dialog::Browser(browser)) = &self.view.dialog {
            if current_folder {
                browser.directory.clone()
            } else {
                browser.chosen_folder().to_owned()
            }
        } else {
            return Ok(());
        };
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(purpose) = browser.palette_file.clone()
        {
            match purpose {
                PaletteFilePicker::Import => {
                    let path = browser
                        .entries
                        .get(browser.selected)
                        .filter(|entry| !entry.directory)
                        .context(AppError::PaletteSelectionRequired)?
                        .path
                        .clone();
                    self.start_palette_import(&path)?;
                    self.view.settings.file_directory = path.parent().map(Path::to_owned);
                    self.cancel_browser();
                    self.show_settings();
                }
                PaletteFilePicker::Export { id, filename } => {
                    let directory = folder;
                    self.view.settings.file_directory = Some(directory.clone());
                    self.cancel_browser();
                    self.text_dialog(
                        TextPurpose::Settings(SettingsEdit::ExportPalette { id, directory }),
                        filename,
                    );
                }
            }
            return Ok(());
        }
        if let Some(Dialog::Browser(browser)) = &self.view.dialog
            && let Some(dialog) = &browser.playlist_name
        {
            let name = folder
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
                folder,
                group_subfolders,
            };
            dialog.selected_all = true;
            self.cancel_browser();
            self.view.dialog = Some(Dialog::Text(dialog));
            return Ok(());
        }
        let paths = if let Some(Dialog::Browser(browser)) = &self.view.dialog {
            if browser.folder {
                vec![folder]
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
