use super::{App, Dialog};
use crate::input::{Key, KeyPress};
use anyhow::{Result, ensure};
use std::path::PathBuf;

#[derive(Clone)]
pub struct ScanFolder {
    pub playlist: i64,
    pub name: String,
    pub folder: PathBuf,
    pub checked: bool,
}

#[derive(Clone, Default)]
pub struct ScanFolders {
    pub rows: Vec<ScanFolder>,
    pub selected: usize,
    pub offset: usize,
    pub capacity: usize,
}

#[derive(Clone, Copy)]
pub enum ScanFolderAction {
    Row(usize),
    All(bool),
    Start,
}

impl App {
    pub fn can_choose_scan_folders(&self) -> bool {
        !self.busy()
            && self
                .library
                .playlists
                .iter()
                .any(|p| p.has_scan_folders())
    }

    pub(super) fn choose_scan_folders(&mut self) -> Result<()> {
        ensure!(
            self.can_choose_scan_folders(),
            "Нет доступных папок плейлистов для поиска."
        );
        let rows = self
            .library
            .playlists
            .iter()
            .flat_map(|p| {
                let playlist = p.id;
                let name = p.display_name(self.settings.preferences.language).to_owned();
                p.scan_folders().into_iter().map(move |folder| ScanFolder {
                    playlist,
                    name: name.clone(),
                    folder,
                    checked: false,
                })
            })
            .collect();
        self.view.dialog = Some(Dialog::ScanFolders(ScanFolders {
            rows,
            ..Default::default()
        }));
        Ok(())
    }

    pub(super) fn scan_folders_scroll(&mut self, delta: i64) {
        if let Some(Dialog::ScanFolders(view)) = &mut self.view.dialog {
            view.selected = super::bounded(view.selected, delta, view.rows.len());
        }
    }

    pub(super) fn scan_folders_action(&mut self, action: ScanFolderAction) -> Result<()> {
        let Some(Dialog::ScanFolders(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        match action {
            ScanFolderAction::Row(index) => {
                if let Some(row) = view.rows.get_mut(index) {
                    view.selected = index;
                    row.checked = !row.checked;
                }
            }
            ScanFolderAction::All(checked) => {
                for row in &mut view.rows {
                    row.checked = checked;
                }
            }
            ScanFolderAction::Start => {
                let mut choices: Vec<(i64, Vec<PathBuf>)> = Vec::new();
                for row in view.rows.iter().filter(|row| row.checked) {
                    if let Some((_, folders)) =
                        choices.iter_mut().find(|(id, _)| *id == row.playlist)
                    {
                        folders.push(row.folder.clone());
                    } else {
                        choices.push((row.playlist, vec![row.folder.clone()]));
                    }
                }
                self.start_playlist_scan(choices, self.view.dialog.clone())?;
                self.view.dialog = None;
            }
        }
        Ok(())
    }

    pub(super) fn scan_folders_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::ScanFolders(view)) = &self.view.dialog else {
            return Ok(());
        };
        let (selected, page) = (view.selected, view.capacity.max(1) as i64);
        match key.key {
            Key::Char(' ') if key.ctrl => self.target(super::Target::ScanSubfolders, false)?,
            Key::Enter => self.scan_folders_action(ScanFolderAction::Start)?,
            Key::Char('a') if key.ctrl => self.scan_folders_action(ScanFolderAction::All(true))?,
            Key::Char('d') if key.ctrl => self.scan_folders_action(ScanFolderAction::All(false))?,
            Key::Char(' ') | Key::Insert => {
                self.scan_folders_action(ScanFolderAction::Row(selected))?
            }
            Key::Up => self.scan_folders_scroll(-1),
            Key::Down => self.scan_folders_scroll(1),
            Key::Home => self.scan_folders_scroll(i64::MIN),
            Key::End => self.scan_folders_scroll(i64::MAX),
            Key::PageUp => self.scan_folders_scroll(-page),
            Key::PageDown => self.scan_folders_scroll(page),
            _ => {}
        }
        Ok(())
    }
}
