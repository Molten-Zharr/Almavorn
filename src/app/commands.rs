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
    Visualizer,
}

impl CommandMenu {
    pub fn title(self, language: crate::model::Language) -> &'static str {
        match self {
            Self::Add => language.text("Add music", "Добавить музыку"),
            Self::Playlist => language.text("Playlist", "Плейлист"),
            Self::Tracks => language.text("Tracks", "Композиции"),
            Self::Player => language.text("Player", "Проигрыватель"),
            Self::Application => language.text("Application", "Приложение"),
            Self::Visualizer => language.text("Background animation", "Анимация фона"),
        }
    }
}

pub struct CommandItem {
    pub label: String,
    pub target: Target,
    pub enabled: bool,
}

impl App {
    pub(super) fn commands_key(&mut self, key: KeyPress) -> Result<()> {
        if self.view.toolbar_selected.is_some() && !key.ctrl && !key.alt {
            if key.key == Key::Tab {
                self.close_dialog();
                self.workspace_key(key)?;
                return Ok(());
            }
            if matches!(key.key, Key::Left | Key::Right) {
                self.close_dialog();
                self.toolbar_key(key)?;
                if let Some(Target::CommandMenu(menu)) = self
                    .view
                    .toolbar_selected
                    .and_then(|index| self.toolbar_items().into_iter().nth(index))
                    .map(|item| item.target)
                {
                    self.open_commands(menu);
                }
                return Ok(());
            }
        }
        let normalized = key.normalized();
        if let Some(action) = self
            .settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.key == normalized)
            .map(|binding| binding.action)
        {
            self.view.dialog = None;
            return self.action(action);
        }
        self.dialog_navigation_key(key)
    }

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

    pub fn edit_caption(&self) -> String {
        if self.selected_playlist_unlocked() {
            self.text("Editing · Done", "Редактирование · Готово")
        } else {
            self.text("Protected · Edit", "Защищен · Редактировать")
        }
        .into()
    }

    pub fn action_caption(&self, action: Action) -> String {
        match action {
            Action::ToggleEdit => self.edit_caption(),
            Action::Undo | Action::Redo => self.history_caption(action == Action::Redo),
            Action::AddFiles | Action::AddFolder | Action::AddNew => format!(
                "{} · {}",
                action.name(self.settings.preferences.language),
                self.import_caption()
            ),
            _ => action.name(self.settings.preferences.language).into(),
        }
    }

    pub fn toolbar_items(&self) -> Vec<CommandItem> {
        let mut entries = Vec::new();
        if !self.command_items(CommandMenu::Add).is_empty() {
            entries.push((
                format!("{} ↓", self.import_caption()),
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
        if self.settings.mode == Mode::Chaos
            && self.control_visible(Panel::PlaylistActions, Action::ComposePlaylists)
        {
            entries.insert(
                0,
                (
                    self.text("Build mix", "Собрать подборку").into(),
                    Target::Action(Action::ComposePlaylists),
                    self.allowed(Action::ComposePlaylists),
                ),
            );
        }
        if self.allowed(Action::ToggleEdit) {
            entries.push((
                self.edit_caption(),
                Target::Action(Action::ToggleEdit),
                true,
            ));
        } else if let Some(playlist) = self
            .playlists()
            .iter()
            .find(|p| self.playlist_unlocked(p.id))
        {
            entries.push((
                format!("{}: {}", self.text("Done", "Готово"), playlist.name),
                Target::FinishEdit,
                !self.busy(),
            ));
        }
        for (action, panel) in [(Action::Undo, Panel::Add), (Action::Redo, Panel::Add)] {
            if self.control_visible(panel, action) {
                entries.push((
                    self.history_caption(action == Action::Redo),
                    Target::Action(action),
                    self.allowed(action),
                ));
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
        if self.control_visible(Panel::Application, Action::Statistics) {
            entries.push((
                self.text("Statistics", "Статистика").into(),
                Target::Action(Action::Statistics),
                true,
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
            format!(
                "{} ↓",
                self.settings
                    .mode
                    .caption(self.settings.preferences.language)
            ),
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
        !self.settings.preferences.workspace.hidden.contains(&panel)
            && !self
                .settings
                .preferences
                .workspace
                .hidden_controls
                .contains(&Control::Action(action))
    }

    pub fn command_items(&self, menu: CommandMenu) -> Vec<CommandItem> {
        use Action::*;
        if menu == CommandMenu::Visualizer {
            return crate::visualizer::VisualizerKind::ALL
                .into_iter()
                .map(|kind| CommandItem {
                    label: format!(
                        "{} {}",
                        if self.settings.preferences.appearance.visualizer == kind {
                            "●"
                        } else {
                            " "
                        },
                        kind.name(self.settings.preferences.language)
                    ),
                    target: Target::Visualizer(kind),
                    enabled: true,
                })
                .collect();
        }
        let (panel, actions): (Panel, &[Action]) = match menu {
            CommandMenu::Visualizer => unreachable!("Animation choices are built above"),
            CommandMenu::Add => (
                Panel::Add,
                &[ToggleDesk, AddFiles, AddFolder, AddFolderPlaylists, AddNew],
            ),
            CommandMenu::Playlist => (
                Panel::PlaylistActions,
                &[
                    NewPlaylist,
                    CopyPlaylist,
                    ComposePlaylists,
                    PlaylistFolders,
                    ToggleEdit,
                    Undo,
                    Redo,
                    Rename,
                    Delete,
                    MoveUp,
                    MoveDown,
                ],
            ),
            CommandMenu::Tracks => (
                Panel::PlaylistActions,
                &[
                    TogglePlay,
                    Mark,
                    PlayMarked,
                    PlayWholePlaylist,
                    Rename,
                    Transfer,
                    Delete,
                    MoveUp,
                    MoveDown,
                    Metadata,
                    EditTags,
                    Sort,
                    Search,
                    Filter,
                ],
            ),
            CommandMenu::Application => (Panel::Application, &[Statistics, Settings, Help, Quit]),
            CommandMenu::Player => (
                Panel::Player,
                &[
                    Previous,
                    TogglePlay,
                    Stop,
                    Next,
                    CycleRepeat,
                    PlayWholePlaylist,
                    ToggleShuffle,
                    Equalizer,
                    PlaybackTuner,
                    CycleVisualizer,
                    VolumeDown,
                    VolumeUp,
                ],
            ),
        };
        let mut items = Vec::new();
        if menu == CommandMenu::Application
            && !self
                .settings
                .preferences
                .workspace
                .hidden
                .contains(&Panel::Application)
        {
            for mode in Mode::ALL {
                if !self
                    .settings
                    .preferences
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
                            mode.caption(self.settings.preferences.language)
                        ),
                        target: Target::Mode(mode),
                        enabled: !self.busy(),
                    });
                }
            }
        }
        for action in actions {
            if *action == EditTags && !self.playlist().is_some_and(|playlist| playlist.kind == crate::model::PlaylistKind::SortingDesk) {
                continue;
            }
            if self.control_visible(panel, *action) {
                let checked = match action {
                    ToggleEdit => None,
                    ToggleDesk => Some(self.settings.preferences.sorting_desk),
                    ToggleShuffle => Some(self.settings.preferences.shuffle),
                    PlayMarked => Some(
                        self.marked_playback_playlist().is_some()
                            && self.library.selected_playlist == self.marked_playback_playlist(),
                    ),
                    _ => None,
                };
                let name = if *action == CycleRepeat {
                    format!(
                        "{}: {}",
                        action.name(self.settings.preferences.language),
                        if self.marked_playback_count().is_some() {
                            self.text("Checked tracks", "Отмеченные")
                        } else {
                            self.settings
                                .preferences
                                .repeat
                                .name(self.settings.preferences.language)
                        },
                    )
                } else {
                    self.action_caption(*action)
                };
                items.push(CommandItem {
                    label: checked.map_or_else(
                        || name.clone(),
                        |value| format!("[{}] {name}", if value { 'x' } else { ' ' }),
                    ),
                    target: if *action == CycleVisualizer {
                        Target::CommandMenu(CommandMenu::Visualizer)
                    } else {
                        Target::Action(*action)
                    },
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
            CommandMenu::Player | CommandMenu::Visualizer => self.focus_panel(Panel::Player),
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
        let selected = if menu == CommandMenu::Visualizer {
            crate::visualizer::VisualizerKind::ALL
                .iter()
                .position(|kind| *kind == self.settings.preferences.appearance.visualizer)
                .unwrap_or_default()
        } else {
            Self::command_selection(&self.command_items(menu), 0, 0)
        };
        self.view.dialog = Some(Dialog::Commands {
            menu,
            selected,
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
