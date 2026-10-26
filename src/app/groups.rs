use super::{App, CommandItem, CommandMenu, Dialog, Target};
use crate::{errors::AppError, input::Action, model::Playlist, workspace::Panel};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(super) struct GroupContext {
    playlist: i64,
    folder: PathBuf,
    anchor: ratatui::layout::Position,
}

#[derive(Clone, Copy)]
pub enum GroupAction {
    Collapse,
    Mark,
    Remove,
    EnableEdit,
}

impl App {
    pub(super) fn open_group_menu(&mut self, folder: PathBuf) {
        let Some(playlist) = self
            .playlist()
            .filter(|playlist| playlist.groups_by_folder())
        else {
            return;
        };
        if !playlist
            .entries
            .iter()
            .any(|entry| entry.track.path.parent().unwrap_or(Path::new("")) == folder)
        {
            return;
        }
        self.view.group_context = Some(GroupContext {
            playlist: playlist.id,
            folder,
            anchor: self.view.pointer,
        });
        self.view.toolbar_selected = None;
        self.focus_panel(Panel::Tracks);
        self.open_commands(CommandMenu::Group);
    }

    fn context_group(&self) -> Option<(&Playlist, &GroupContext, Vec<i64>)> {
        let context = self.view.group_context.as_ref()?;
        let playlist = self
            .playlist()
            .filter(|playlist| playlist.id == context.playlist && playlist.groups_by_folder())?;
        let entries: Vec<_> = playlist
            .entries
            .iter()
            .filter(|entry| entry.track.path.parent().unwrap_or(Path::new("")) == context.folder)
            .map(|entry| entry.id)
            .collect();
        (!entries.is_empty()).then_some((playlist, context, entries))
    }

    pub(super) fn group_items(&self) -> Vec<CommandItem> {
        let Some((playlist, context, entries)) = self.context_group() else {
            return Vec::new();
        };
        let marked = entries.iter().all(|id| self.library.marked.contains(id));
        let mut items = vec![
            CommandItem {
                label: format!(
                    "{} · {}",
                    playlist.folder_group_name(&context.folder),
                    entries.len()
                ),
                target: Target::MenuSection,
                enabled: false,
            },
            CommandItem {
                label: if self.track_group_collapsed(&context.folder) {
                    self.text("Expand group", "Развернуть группу")
                } else {
                    self.text("Collapse group", "Свернуть группу")
                }
                .into(),
                target: Target::GroupAction(GroupAction::Collapse),
                enabled: true,
            },
            CommandItem {
                label: if marked {
                    self.text("Uncheck all in group", "Снять отметки со всей группы")
                } else {
                    self.text("Check all in group", "Отметить всю группу")
                }
                .into(),
                target: Target::GroupAction(GroupAction::Mark),
                enabled: !self.busy(),
            },
        ];
        if !self.playlist_editable(playlist) {
            items.push(CommandItem {
                label: self
                    .text(
                        "Enable playlist editing",
                        "Включить редактирование плейлиста",
                    )
                    .into(),
                target: Target::GroupAction(GroupAction::EnableEdit),
                enabled: self.allowed(Action::ToggleEdit),
            });
        }
        items.push(CommandItem {
            label: self
                .text("Remove group from playlist…", "Убрать группу из плейлиста…")
                .into(),
            target: Target::GroupAction(GroupAction::Remove),
            enabled: !self.busy() && self.playlist_editable(playlist),
        });
        items
    }

    pub(super) fn group_action(&mut self, action: GroupAction) -> Result<()> {
        let (playlist, context, entries) = self.context_group().context(AppError::GroupChanged)?;
        let editable = self.playlist_editable(playlist);
        let name = playlist.folder_group_name(&context.folder);
        let context = context.clone();
        if !matches!(action, GroupAction::Collapse) {
            ensure!(!self.busy(), AppError::LibraryBusy);
        }
        match action {
            GroupAction::Collapse => self.toggle_track_group(&context.folder),
            GroupAction::Mark => {
                let marked = !entries.iter().all(|id| self.library.marked.contains(id));
                self.set_track_marks(&entries, marked)?;
            }
            GroupAction::EnableEdit => {
                if !editable {
                    self.action(Action::ToggleEdit)?;
                }
                self.view.pointer = context.anchor;
                self.open_commands(CommandMenu::Group);
            }
            GroupAction::Remove => {
                ensure!(editable, AppError::OrderProtected);
                self.view.dialog = Some(Dialog::RemoveGroup {
                    playlist: context.playlist,
                    folder: context.folder,
                    name,
                    entries,
                });
            }
        }
        Ok(())
    }
}
