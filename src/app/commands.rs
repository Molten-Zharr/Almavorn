use super::{App, Dialog, Target};
use crate::{
    input::{Action, Key, KeyPress},
    model::Mode,
    workspace::{Control, Panel},
};
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandMenu {
    Add,
    Playlist,
    Tracks,
    Player,
    Application,
}

impl CommandMenu {
    pub fn title(self, language: crate::model::Language) -> &'static str {
        match self {
            Self::Add => language.text("Add music", "Добавить музыку"),
            Self::Playlist => language.text("Playlist", "Плейлист"),
            Self::Tracks => language.text("Tracks", "Композиции"),
            Self::Player => language.text("Player", "Проигрыватель"),
            Self::Application => language.text("Application", "Приложение"),
        }
    }
}

pub struct CommandItem {
    pub label: String,
    pub target: Target,
    pub enabled: bool,
}

impl App {
    pub(crate) fn command_selection(
        items: &[CommandItem],
        selected: usize,
        direction: i64,
    ) -> usize {
        let enabled: Vec<_> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.enabled.then_some(index))
            .collect();
        let current = enabled
            .iter()
            .position(|index| *index == selected)
            .unwrap_or_else(|| {
                enabled
                    .partition_point(|index| *index < selected)
                    .min(enabled.len().saturating_sub(1))
            });
        enabled
            .get(super::bounded(current, direction, enabled.len()))
            .copied()
            .unwrap_or(0)
    }

    pub fn toolbar_items(&self) -> Vec<CommandItem> {
        let mut entries = Vec::new();
        if !self.command_items(CommandMenu::Add).is_empty() {
            entries.push((
                self.text("Add ↓", "Добавить ↓").into(),
                Target::CommandMenu(CommandMenu::Add),
                true,
            ));
        }
        if !self.command_items(CommandMenu::Playlist).is_empty() {
            entries.push((
                self.text("Playlist ↓", "Плейлист ↓").into(),
                Target::CommandMenu(CommandMenu::Playlist),
                true,
            ));
        }
        for (action, label, panel) in [
            (Action::Undo, "←", Panel::Add),
            (Action::Redo, "→", Panel::Add),
        ] {
            if self.control_visible(panel, action) {
                entries.push((label.into(), Target::Action(action), self.allowed(action)));
            }
        }
        if self.control_visible(Panel::PlaylistActions, Action::Search) {
            entries.push((
                self.text("Search…", "Поиск…").into(),
                Target::Action(Action::Search),
                true,
            ));
        }
        if self.control_visible(Panel::PlaylistActions, Action::Filter) {
            entries.push((
                self.text("Filter", "Фильтр").into(),
                Target::Action(Action::Filter),
                self.allowed(Action::Filter),
            ));
        }
        if self.control_visible(Panel::Application, Action::Settings) {
            entries.push((
                self.text("Settings", "Настройки").into(),
                Target::Action(Action::Settings),
                true,
            ));
        }
        entries.push((
            if self.view.layout_editing {
                self.text("Done", "Готово")
            } else {
                self.text("Layout", "Блоки")
            }
            .into(),
            Target::Action(Action::Panels),
            true,
        ));
        if self.view.layout_editing {
            entries.push((
                self.text("Visibility…", "Видимость…").into(),
                Target::ManagePanels,
                true,
            ));
        }
        entries.push((
            format!("{} ↓", self.settings.mode.name(self.settings.language)),
            Target::CommandMenu(CommandMenu::Application),
            true,
        ));
        entries
            .into_iter()
            .map(|(label, target, enabled)| CommandItem {
                label,
                target,
                enabled,
            })
            .collect()
    }

    pub fn control_visible(&self, panel: Panel, action: Action) -> bool {
        !self.settings.workspace.hidden.contains(&panel)
            && !self
                .settings
                .workspace
                .hidden_controls
                .contains(&Control::Action(action))
    }

    pub fn command_items(&self, menu: CommandMenu) -> Vec<CommandItem> {
        use Action::*;
        let (panel, actions): (Panel, &[Action]) = match menu {
            CommandMenu::Add => (Panel::Add, &[AddFiles, AddFolder, AddNew]),
            CommandMenu::Playlist => (
                Panel::PlaylistActions,
                &[
                    NewPlaylist,
                    CopyPlaylist,
                    ComposePlaylists,
                    PlaylistFolders,
                    Rename,
                    Delete,
                    MoveUp,
                    MoveDown,
                ],
            ),
            CommandMenu::Tracks => (
                Panel::PlaylistActions,
                &[
                    TogglePlay, Mark, Rename, Transfer, Delete, MoveUp, MoveDown, Metadata, Sort,
                    Search, Filter,
                ],
            ),
            CommandMenu::Application => (
                Panel::Application,
                &[ToggleEdit, ToggleDesk, Settings, Help, Quit],
            ),
            CommandMenu::Player => (
                Panel::Player,
                &[
                    Previous,
                    TogglePlay,
                    Stop,
                    Next,
                    CycleRepeat,
                    ToggleShuffle,
                    VolumeDown,
                    VolumeUp,
                ],
            ),
        };
        let mut items = Vec::new();
        if menu == CommandMenu::Application
            && !self.settings.workspace.hidden.contains(&Panel::Application)
        {
            for mode in Mode::ALL {
                if !self
                    .settings
                    .workspace
                    .hidden_controls
                    .contains(&Control::Mode(mode))
                {
                    items.push(CommandItem {
                        label: format!(
                            "{} {}",
                            if self.settings.mode == mode {
                                "●"
                            } else {
                                " "
                            },
                            mode.name(self.settings.language)
                        ),
                        target: Target::Mode(mode),
                        enabled: !self.busy(),
                    });
                }
            }
        }
        for action in actions {
            if self.control_visible(panel, *action) {
                let checked = match action {
                    ToggleEdit => Some(self.view.editing),
                    ToggleDesk => Some(self.settings.sorting_desk),
                    ToggleShuffle => Some(self.settings.shuffle),
                    _ => None,
                };
                let name = if *action == CycleRepeat {
                    format!(
                        "{}: {}",
                        action.name(self.settings.language),
                        self.settings.repeat.name(self.settings.language),
                    )
                } else {
                    action.name(self.settings.language).into()
                };
                items.push(CommandItem {
                    label: checked.map_or_else(
                        || name.clone(),
                        |value| format!("[{}] {name}", if value { 'x' } else { ' ' }),
                    ),
                    target: Target::Action(*action),
                    enabled: self.allowed(*action),
                });
            }
        }
        if menu == CommandMenu::Application {
            items.push(CommandItem {
                label: self.text("Blocks and buttons…", "Блоки и кнопки…").into(),
                target: Target::ManagePanels,
                enabled: true,
            });
        }
        items
    }

    pub(crate) fn open_commands(&mut self, menu: CommandMenu) {
        let toolbar_selected = self.view.toolbar_selected;
        let toolbar_anchor = toolbar_selected
            .and_then(|index| self.view.toolbar_areas.get(index))
            .map(|area| area.as_position());
        match menu {
            CommandMenu::Playlist => self.focus_panel(Panel::Playlists),
            CommandMenu::Tracks => self.focus_panel(Panel::Tracks),
            CommandMenu::Player => self.focus_panel(Panel::Player),
            _ => {}
        }
        self.view.toolbar_selected = toolbar_selected;
        let anchor = if let Some(anchor) = toolbar_anchor {
            anchor
        } else if self.view.pointer.x == u16::MAX || self.view.pointer.y == u16::MAX {
            self.view
                .workspace
                .areas
                .iter()
                .find(|(panel, _)| *panel == self.view.workspace.focus)
                .map_or(ratatui::layout::Position::new(0, 1), |(_, area)| {
                    area.as_position()
                })
        } else {
            self.view.pointer
        };
        self.view.dialog = Some(Dialog::Commands {
            menu,
            selected: Self::command_selection(&self.command_items(menu), 0, 0),
            anchor,
        });
    }

    pub(crate) fn focus_toolbar(&mut self, last: bool) {
        let items = self.toolbar_items();
        self.view.toolbar_selected = if last {
            items.iter().rposition(|item| item.enabled)
        } else {
            items.iter().position(|item| item.enabled)
        };
    }

    fn move_toolbar(&mut self, direction: i32) {
        let items = self.toolbar_items();
        let enabled: Vec<_> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| item.enabled.then_some(index))
            .collect();
        if enabled.is_empty() {
            self.view.toolbar_selected = None;
            return;
        }
        let current = enabled
            .iter()
            .position(|index| Some(*index) == self.view.toolbar_selected)
            .unwrap_or(0);
        let next = (current as i32 + direction).rem_euclid(enabled.len() as i32) as usize;
        self.view.toolbar_selected = Some(enabled[next]);
    }

    pub(crate) fn toolbar_key(&mut self, key: KeyPress) -> Result<bool> {
        if self.view.toolbar_selected.is_none() || key.ctrl || key.alt {
            return Ok(false);
        }
        match key.key {
            Key::Left | Key::Up => self.move_toolbar(-1),
            Key::Right | Key::Down => self.move_toolbar(1),
            Key::Home | Key::PageUp => self.focus_toolbar(false),
            Key::End | Key::PageDown => self.focus_toolbar(true),
            Key::Enter | Key::Char(' ') | Key::F(10) => {
                if let Some(item) = self
                    .view
                    .toolbar_selected
                    .and_then(|index| self.toolbar_items().into_iter().nth(index))
                    .filter(|item| item.enabled)
                {
                    self.target(item.target, false)?;
                }
            }
            _ => return Ok(false),
        }
        self.view.pointer = ratatui::layout::Position::new(u16::MAX, u16::MAX);
        Ok(true)
    }

    pub(crate) fn activate_command(&mut self, index: usize) -> Result<()> {
        let Some(Dialog::Commands { menu, .. }) = &self.view.dialog else {
            return Ok(());
        };
        let Some(item) = self
            .command_items(*menu)
            .into_iter()
            .nth(index)
            .filter(|item| item.enabled)
        else {
            return Ok(());
        };
        self.view.dialog = None;
        self.target(item.target, false)
    }
}
