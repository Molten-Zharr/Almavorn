use super::{App, CommandItem, CommandMenu, Dialog, Target};
use crate::{input::{Action, Key, KeyPress}, model::{Mode, RepeatMode}, toolbar::{QuickAction, QuickButton}};
use anyhow::Result;
use ratatui::layout::Rect;

impl App {
    pub(crate) fn quick_active(&self, button: QuickButton) -> bool {
        use QuickButton::*;
        match button {
            Mode => self.settings.mode == crate::model::Mode::Chaos,
            Edit => self.playlists().iter().any(|p| self.playlist_unlocked(p.id)),
            Desk => self.settings.mode == crate::model::Mode::Chaos && self.settings.preferences.sorting_desk,
            Shuffle => self.settings.preferences.shuffle,
            Repeat => self.settings.preferences.repeat != RepeatMode::Off,
            Marked => self.marked_playback_playlist().is_some(),
            Lyrics => self.settings.preferences.lyrics.show_panel,
            ArtworkSearch => self.settings.preferences.artwork.search_online,
            SpecialButtons => self.settings.preferences.appearance.show_player_special_buttons,
            Layout => self.view.layout_editing,
            Equalizer => self.settings.preferences.equalizer.enabled,
            Effects => self.tuner_settings() != &Default::default(),
            Animation => self.settings.preferences.appearance.visualizer != crate::visualizer::VisualizerKind::Off,
            Filter => self.view.filter_editing || !self.library.query.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn quick_item(&self, button: QuickButton) -> CommandItem {
        use QuickButton::*;
        let language = self.settings.preferences.language;
        let target = match button {
            Add => Target::CommandMenu(CommandMenu::Add),
            Playlist => Target::CommandMenu(CommandMenu::Playlist),
            Tracks => Target::CommandMenu(CommandMenu::Tracks),
            Settings => Target::Action(Action::Settings),
            Mode => Target::Action(Action::SwitchMode),
            Edit if self.quick_active(Edit) => Target::FinishEdit,
            Edit => Target::Action(Action::ToggleEdit),
            Desk => Target::Action(Action::ToggleDesk),
            Shuffle => Target::Action(Action::ToggleShuffle),
            Repeat => Target::Action(Action::CycleRepeat),
            Marked if self.quick_active(Marked) => Target::Action(Action::PlayWholePlaylist),
            Marked => Target::Action(Action::PlayMarked),
            Lyrics => Target::ToggleLyricsPanel,
            SpecialButtons => Target::TogglePlayerSpecialButtons,
            Layout => Target::Action(Action::Panels),
            Equalizer => Target::Action(Action::Equalizer),
            Effects => Target::Action(Action::PlaybackTuner),
            Animation => Target::CommandMenu(CommandMenu::Visualizer),
            Compose => Target::Action(Action::ComposePlaylists),
            Undo => Target::Action(Action::Undo),
            Redo => Target::Action(Action::Redo),
            Search => Target::Action(Action::Search),
            Filter => Target::Action(Action::Filter),
            Services => Target::Action(Action::Services),
            ArtworkSearch => Target::ToggleArtworkSearch,
            Downloads => Target::Action(Action::Downloads),
            Mods => Target::Action(Action::Mods),
            Statistics => Target::Action(Action::Statistics),
            Help => Target::Action(Action::Help),
            Quit => Target::Action(Action::Quit),
            Visibility => Target::ManagePanels,
            Customize => Target::QuickAccess(QuickAction::Open),
        };
        let name = match button {
            Add => format!("{} ↓", self.import_caption()),
            Playlist | Tracks => format!("{} ↓", button.name(language)),
            Mode => self.settings.mode.name(language).into(),
            Edit => self.text("Edit", "Правка").into(),
            Desk => self.text("To desk", "На стол").into(),
            Shuffle => self.text("Shuffle", "Вразброс").into(),
            Repeat => format!("{}: {}", self.text("Repeat", "Повтор"), self.settings.preferences.repeat.name(language)),
            Lyrics => self.text("Lyrics", "Текст песни").into(),
            SpecialButtons => self.text("Special buttons", "Спецкнопки").into(),
            Layout => self.text("Edit layout", "Редактирование блоков").into(),
            _ => button.name(language).into(),
        };
        let label = if matches!(button, Layout | ArtworkSearch) {
            format!("[{}] {name}", if self.quick_active(button) { 'x' } else { ' ' })
        } else if self.quick_active(button) { format!("[x] {name}") } else { name };
        let enabled = match target {
            Target::Action(action) => self.allowed(action),
            Target::FinishEdit => true,
            _ => true,
        };
        CommandItem { label, target, enabled }
    }

    // Layout and overflow share this calculation, including buttons that do not fit.
    pub(crate) fn quick_layout(&self) -> (Vec<(QuickButton, Rect)>, Option<Rect>, u16) {
        let settings = &self.settings.preferences.appearance.quick_access;
        let width = self.view.toolbar_width;
        let max_rows = self.view.toolbar_max_rows.max(1);
        let mut x = 11u16.min(width);
        let mut y = 0u16;
        let mut placed = Vec::new();
        for button in QuickButton::ALL {
            let show_layout = settings.show_active && button == QuickButton::Layout && self.view.layout_editing;
            if !settings.pinned.contains(&button) && !show_layout { continue; }
            let item = self.quick_item(button);
            let length = crate::layout::buttons::button_width(self, &item.label, &item.target).min(width);
            if x.saturating_add(length) > width { x = 0; y += 1; }
            if y >= max_rows { break; }
            placed.push((button, Rect::new(x, y, length, 1)));
            x = x.saturating_add(length + 1);
        }
        let more = if placed.len() < QuickButton::ALL.len() {
            let length = crate::layout::buttons::button_width(self, self.text("More…", "Больше…"), &Target::CommandMenu(CommandMenu::More)).min(width);
            if x.saturating_add(length) > width { x = 0; y += 1; }
            if y >= max_rows {
                y = max_rows - 1;
                while placed.last().is_some_and(|(_, rect)| rect.y == y && rect.right().saturating_add(length + 1) > width) {
                    placed.pop();
                }
                x = placed.last().filter(|(_, rect)| rect.y == y).map_or(if y == 0 { 11.min(width.saturating_sub(length)) } else { 0 }, |(_, rect)| rect.right() + 1);
            }
            Some(Rect::new(x, y, length.min(width.saturating_sub(x)), 1))
        } else { None };
        let height = placed.last().map_or(1, |(_, rect)| rect.bottom()).max(more.map_or(0, |rect| rect.bottom()));
        (placed, more, height)
    }

    pub fn toolbar_items(&self) -> Vec<CommandItem> {
        let (placed, more, _) = self.quick_layout();
        let mut items: Vec<_> = placed.into_iter().map(|(button, _)| self.quick_item(button)).collect();
        if more.is_some() { items.push(CommandItem { label: self.text("More…", "Больше…").into(), target: Target::CommandMenu(CommandMenu::More), enabled: true }); }
        items
    }

    pub(super) fn more_items(&self) -> Vec<CommandItem> {
        use QuickButton as B;
        let (placed, _, _) = self.quick_layout();
        let mut items = Vec::new();
        for (en, ru, buttons) in [
            ("Utilities (Application)", "Служебные (Приложение)", &[B::Mode, B::Settings, B::Layout, B::Visibility, B::Customize, B::Statistics, B::Help, B::Quit][..]),
            ("Editing (Playlist)", "Редактирование (Плейлист)", &[B::Playlist, B::Edit, B::Compose, B::Undo, B::Redo][..]),
            ("Management (Tracks)", "Управление (Композиции)", &[B::Add, B::Tracks, B::Desk, B::Search, B::Filter, B::Marked, B::Lyrics][..]),
            ("Playback (Player)", "Воспроизведение (Проигрыватель)", &[B::Shuffle, B::Repeat, B::Equalizer, B::Effects, B::Animation, B::SpecialButtons][..]),
            ("Services (Application)", "Сервисы (Приложение)", &[B::Services, B::ArtworkSearch, B::Downloads, B::Mods][..]),
        ] {
            let mut group = Vec::new();
            for &button in buttons {
                if placed.iter().any(|(value, _)| *value == button) { continue; }
                if button == B::Mode {
                    for mode in Mode::ALL {
                        group.push(CommandItem { label: format!("{} {}", if self.settings.mode == mode { "●" } else { " " }, mode.caption(self.settings.preferences.language)), target: Target::Mode(mode), enabled: !self.busy() });
                    }
                } else { group.push(self.quick_item(button)); }
            }
            if group.is_empty() { continue; }
            if !items.is_empty() { items.push(CommandItem { label: String::new(), target: Target::MenuSection, enabled: false }); }
            items.push(CommandItem { label: self.text(en, ru).into(), target: Target::MenuSection, enabled: false });
            items.extend(group);
        }
        if items.is_empty() { items.push(self.quick_item(QuickButton::Customize)); }
        items
    }

    pub(crate) fn quick_access_action(&mut self, action: QuickAction) -> Result<()> {
        let settings = &mut self.settings.preferences.appearance.quick_access;
        match action {
            QuickAction::Open => { self.view.dialog = Some(Dialog::QuickAccess { selected: 0 }); return Ok(()); }
            QuickAction::Toggle(index) => if let Some(button) = QuickButton::ALL.get(index) {
                if settings.pinned.contains(button) { settings.pinned.retain(|value| value != button); }
                else { settings.pinned.push(*button); }
                if let Some(Dialog::QuickAccess { selected }) = &mut self.view.dialog { *selected = index; }
            },
            QuickAction::All(all) => { settings.pinned = if all { QuickButton::ALL.to_vec() } else { Vec::new() }; settings.show_active = all; }
            QuickAction::Reset => *settings = Default::default(),
            QuickAction::Active => settings.show_active = !settings.show_active,
        }
        self.view.toolbar_selected = None;
        self.save_settings()
    }

    pub(super) fn quick_access_key(&mut self, key: KeyPress) -> Result<()> {
        let key = key.normalized();
        let Some(Dialog::QuickAccess { selected }) = self.view.dialog else { return Ok(()); };
        match (key.ctrl, key.alt, key.key) {
            (true, false, Key::Char('a')) => self.quick_access_action(QuickAction::All(true)),
            (true, false, Key::Char('d')) => self.quick_access_action(QuickAction::All(false)),
            (true, false, Key::Char(' ')) => self.quick_access_action(QuickAction::Active),
            (true, false, Key::Char('r')) => self.quick_access_action(QuickAction::Reset),
            (false, false, Key::Home | Key::End) => {
                self.view.dialog = Some(Dialog::QuickAccess { selected: if key.key == Key::Home { 0 } else { QuickButton::ALL.len() - 1 } });
                Ok(())
            }
            (false, false, Key::Enter | Key::Char(' ')) => self.quick_access_action(QuickAction::Toggle(selected)),
            _ => self.dialog_navigation_key(key),
        }
    }
}
