use super::{App, Dialog, FormField, FormPurpose, database::DatabaseOutcome};
use crate::{
    input::{Action, Key, KeyPress},
    model::PlaylistKind,
    tags::Document,
};
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, path::PathBuf};

#[derive(Clone)]
pub struct MoveFiles {
    pub desk: i64,
    pub ids: Vec<i64>,
    pub playlist: i64,
    pub name: String,
    pub folders: Vec<PathBuf>,
    pub selected: usize,
    pub confirm: bool,
}

impl App {
    pub(super) fn edit_transfer_destination(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Дождитесь завершения текущей операции.");
        let Some(Dialog::Transfer { selected, .. }) = &self.view.dialog else {
            return Ok(());
        };
        let playlist = self
            .transfer_destinations()
            .get(*selected)
            .copied()
            .context("Выберите плейлист назначения.")?;
        if playlist.mode == crate::model::Mode::Order && playlist.kind == PlaylistKind::Normal {
            self.view.editing_playlist = Some(playlist.id);
        }
        Ok(())
    }

    pub(super) fn open_move_files(&mut self) -> Result<()> {
        ensure!(
            !self.busy() && !self.writing_tags(),
            "Дождитесь завершения текущей операции."
        );
        let Some(Dialog::Transfer { ids, selected }) = &self.view.dialog else {
            return Ok(());
        };
        let desk = self.playlist().context("Выберите сортировочный стол.")?;
        ensure!(
            desk.kind == PlaylistKind::SortingDesk,
            "Перенос файлов доступен на сортировочном столе."
        );
        let destination = self
            .transfer_destinations()
            .get(*selected)
            .copied()
            .context("Выберите плейлист назначения.")?;
        ensure!(
            destination.kind == PlaylistKind::Normal,
            "Выберите обычный плейлист."
        );
        ensure!(
            self.playlist_editable(destination),
            crate::errors::AppError::OrderProtected
        );
        let mut folders = destination.folders.clone();
        folders.sort();
        folders.dedup();
        self.view.dialog = Some(Dialog::MoveFiles(MoveFiles {
            desk: desk.id,
            ids: ids.clone(),
            playlist: destination.id,
            name: destination.name.clone(),
            folders,
            selected: 0,
            confirm: false,
        }));
        Ok(())
    }

    pub(super) fn choose_move_folder(&mut self) {
        let Some(Dialog::MoveFiles(view)) = &self.view.dialog else {
            return;
        };
        let folder = view
            .folders
            .get(view.selected)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let purpose = FormPurpose::MoveFilesFolder(view.clone());
        self.open_form(
            self.text("Destination folder", "Папка назначения"),
            self.text(
                "Enter an existing folder. It will be linked to the playlist after the move.",
                "Укажите существующую папку. После переноса она будет связана с плейлистом.",
            ),
            vec![FormField::new(self.text("Folder", "Папка"), folder)],
            purpose,
        );
    }

    pub(super) fn close_move_files(&mut self) -> bool {
        let Some(Dialog::MoveFiles(view)) = &mut self.view.dialog else {
            return false;
        };
        if self.tags.saving {
            return true;
        }
        if view.confirm {
            view.confirm = false;
        } else {
            let (ids, playlist) = (view.ids.clone(), view.playlist);
            let selected = self
                .transfer_destinations()
                .iter()
                .position(|p| p.id == playlist)
                .unwrap_or(0);
            self.view.dialog = Some(Dialog::Transfer { ids, selected });
        }
        true
    }

    pub(super) fn transfer_key(&mut self, key: KeyPress) -> Result<()> {
        if self.writing_tags() {
            return Ok(());
        }
        let key = key.normalized();
        if matches!(self.view.dialog, Some(Dialog::Transfer { .. })) {
            if self
                .settings
                .preferences
                .bindings
                .iter()
                .any(|b| b.action == Action::ToggleEdit && b.key == key.normalized())
            {
                return self.edit_transfer_destination();
            }
            if key.ctrl && key.key == Key::Enter {
                return self.open_move_files();
            }
        }
        if matches!(self.view.dialog.as_ref(), Some(Dialog::MoveFiles(view)) if !view.confirm)
            && key.ctrl
            && key.key == Key::Char('f')
        {
            self.choose_move_folder();
            return Ok(());
        }
        match key.key {
            Key::Home => self.scroll_dialog(i16::MIN),
            Key::End => self.scroll_dialog(i16::MAX),
            _ => self.dialog_navigation_key(key)?,
        }
        Ok(())
    }

    pub(super) fn submit_move_files(&mut self) -> Result<()> {
        ensure!(
            !self.busy() && !self.writing_tags() && !self.settings_saving(),
            "Дождитесь завершения текущей операции."
        );
        ensure!(
            !self.tags.recovery_required,
            "Перезапустите плеер для восстановления предыдущей записи файла."
        );
        let Some(Dialog::MoveFiles(view)) = &mut self.view.dialog else {
            return Ok(());
        };
        let folder = view
            .folders
            .get(view.selected)
            .context("Выберите папку назначения.")?
            .clone();
        if !view.confirm {
            view.confirm = true;
            return Ok(());
        }
        let mut recovery = view.clone();
        recovery.confirm = false;
        let (desk, destination, ids) = (view.desk, view.playlist, view.ids.clone());
        let playlist = self
            .library
            .playlists
            .iter()
            .find(|p| p.id == destination)
            .context("Плейлист больше не существует.")?;
        ensure!(
            self.playlist_editable(playlist),
            crate::errors::AppError::OrderProtected
        );
        let source = self
            .library
            .playlists
            .iter()
            .find(|p| p.id == desk && p.kind == PlaylistKind::SortingDesk)
            .context("Сортировочный стол больше не существует.")?;
        let mut seen = HashSet::new();
        let tracks = ids
            .iter()
            .filter(|id| seen.insert(**id))
            .map(|id| {
                source
                    .entries
                    .iter()
                    .find(|e| e.track.id == *id)
                    .map(|e| e.track.clone())
                    .context("Композиция больше не находится на сортировочном столе.")
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(!tracks.is_empty(), "Выберите композиции.");
        let unlocked = self.playlist_unlocked(destination);
        self.start_database(Some(Dialog::MoveFiles(recovery)), move |store| {
            let folder = folder
                .canonicalize()
                .context("Не удалось открыть папку назначения.")?;
            ensure!(folder.is_dir(), "Выберите существующую папку.");
            let mut paths = HashSet::new();
            // Check all name collisions before moving the first file.
            for track in &tracks {
                let path = folder.join(track.path.file_name().context("У файла нет имени.")?);
                ensure!(
                    paths.insert(path.clone()),
                    "Среди выбранных файлов повторяется имя: {}",
                    path.display()
                );
                ensure!(
                    path == track.path || !path.try_exists()?,
                    "Файл уже существует: {}",
                    path.display()
                );
            }
            let mut outcomes = Vec::new();
            let mut failures = Vec::new();
            let mut processed = 0;
            // Each successful move prepends a track; reverse processing preserves the selection order.
            for track in tracks.iter().rev() {
                let result = (|| {
                    let document = Document::read(&track.path)?;
                    let path = folder.join(track.path.file_name().context("У файла нет имени.")?);
                    store.edit_audio_file(
                        desk,
                        track.id,
                        &document,
                        &path,
                        Some(destination),
                        unlocked,
                    )
                })();
                processed += 1;
                match result {
                    Ok(outcome) => outcomes.push(outcome),
                    Err(error) => failures.push(super::tags::batch_file_error(
                        &track.path,
                        &format!("{error:#}"),
                    )),
                }
                if crate::tags::recovery_path(&store.path).exists() {
                    break;
                }
            }
            Ok(DatabaseOutcome::FilesMoved {
                destination,
                outcomes,
                failures,
                remaining: tracks.len() - processed,
            })
        })?;
        self.tags.saving = true;
        Ok(())
    }
}
