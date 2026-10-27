use super::{App, Dialog, SettingsFocus, SettingsPage, Target};
use crate::{
    app::{ModsView, ServicePage, ServicesView},
    input::{Key, KeyPress},
};

impl App {
    pub(crate) fn settings_expanded(&self, section: SettingsPage) -> bool {
        match section {
            SettingsPage::Themes => self.view.settings.themes_expanded,
            SettingsPage::Mods => self.view.settings.mods_expanded,
            SettingsPage::Services => self.view.settings.services_expanded,
            _ => false,
        }
    }

    pub(super) fn set_settings_expanded(&mut self, section: SettingsPage, expanded: bool) {
        match section {
            SettingsPage::Themes => self.view.settings.themes_expanded = expanded,
            SettingsPage::Mods => self.view.settings.mods_expanded = expanded,
            SettingsPage::Services => self.view.settings.services_expanded = expanded,
            _ => {}
        }
    }

    pub(super) fn selected_settings_group(&self) -> Option<SettingsPage> {
        match self
            .settings_menu_targets()
            .get(self.view.settings.menu_selected)
        {
            Some(Target::SettingsThemes) => Some(SettingsPage::Themes),
            Some(Target::SettingsGroup(page)) => Some(*page),
            _ => None,
        }
    }

    pub(super) fn focus_settings_group(&mut self, section: SettingsPage) {
        self.view.settings.menu_selected = self
            .settings_menu_targets()
            .iter()
            .position(|target| {
                matches!(target, Target::SettingsGroup(page) if *page == section)
                    || (section == SettingsPage::Themes && matches!(target, Target::SettingsThemes))
            })
            .unwrap_or(0);
        self.view.settings.focus = SettingsFocus::Menu;
    }

    pub(in crate::app) fn toggle_settings_group(&mut self, section: SettingsPage) {
        if section.tabs().is_empty() {
            return;
        }
        let expanded = !self.settings_expanded(section);
        if expanded && self.view.settings.page.section() != section {
            self.open_settings_page(section);
        }
        self.set_settings_expanded(section, expanded);
        self.focus_settings_group(section);
    }

    pub(super) fn remember_service_selection(&mut self) {
        if let Some(Dialog::Services(view)) = &self.view.dialog
            && view.page == ServicePage::Connections
            && let Some(connection) = self.services.connections.get(view.selected)
        {
            let id = connection.id.clone();
            self.services.select_connection(id);
        }
    }

    pub(super) fn show_online_settings(&mut self) -> bool {
        let page = match self.view.settings.page {
            SettingsPage::Mods => {
                self.view.dialog = Some(Dialog::Mods(ModsView {
                    install: self.mods.pending_install.take(),
                    ..Default::default()
                }));
                self.ensure_mods_loaded();
                return true;
            }
            SettingsPage::Services => ServicePage::Connections,
            SettingsPage::ServiceTracks => ServicePage::Tracks,
            SettingsPage::ServicePlaylists => ServicePage::Playlists,
            SettingsPage::ServiceDevices => ServicePage::Devices,
            _ => return false,
        };
        let selected = if page == ServicePage::Connections {
            self.services
                .connections
                .iter()
                .position(|c| Some(&c.id) == self.services.connection.as_ref())
                .unwrap_or(0)
        } else {
            0
        };
        self.view.dialog = Some(Dialog::Services(ServicesView {
            page,
            selected,
            ..Default::default()
        }));
        self.ensure_services_loaded();
        true
    }

    pub(crate) fn sync_online_settings(&mut self) {
        let page = match self.view.dialog.as_ref().map(Dialog::settings_base) {
            Some(Dialog::Mods(_)) => SettingsPage::Mods,
            Some(Dialog::Services(view)) => match view.page {
                ServicePage::Connections => SettingsPage::Services,
                ServicePage::Tracks => SettingsPage::ServiceTracks,
                ServicePage::Playlists => SettingsPage::ServicePlaylists,
                ServicePage::Devices => SettingsPage::ServiceDevices,
            },
            _ => return,
        };
        if self.view.settings.page != page {
            self.view.settings.page = page;
            self.sync_settings_menu();
        }
    }

    pub(crate) fn settings_guide(&self) -> Option<&'static str> {
        match self.view.settings.page {
            SettingsPage::ModGuide => Some(self.text(
                include_str!("../../../docs/MODS.md"),
                include_str!("../../../docs/MODS.ru.md"),
            )),
            SettingsPage::ServiceGuide => self.service_guide_topics().get(self.view.settings.guide_topic).map(|(_, text)| *text),
            _ => None,
        }
    }

    pub(crate) fn service_guide_topics(&self) -> Vec<(&'static str, &'static str)> {
        let document = self.text(include_str!("../../../docs/ONLINE_MUSIC.md"), include_str!("../../../docs/ONLINE_MUSIC.ru.md"));
        document.split("\n## ").enumerate().map(|(index, section)| {
            let (title, text) = section.split_once('\n').unwrap_or((section, ""));
            (if index == 0 { self.text("Overview", "Обзор") } else { title }, text.trim())
        }).collect()
    }

    pub(in crate::app) fn select_settings_guide_topic(&mut self, index: usize) {
        if self.view.settings.page != SettingsPage::ServiceGuide { return; }
        self.view.settings.guide_topic = index.min(self.service_guide_topics().len().saturating_sub(1));
        self.view.settings.offset = 0;
        self.view.settings.focus = SettingsFocus::Parameters;
    }

    pub(super) fn scroll_online_settings(&mut self, direction: i64) -> bool {
        match self.view.dialog {
            Some(Dialog::Mods(_)) => self.mods_scroll(direction),
            Some(Dialog::Services(_)) => self.services_scroll(direction),
            _ if self.settings_guide().is_some() => {
                self.view.settings.offset = crate::app::bounded(
                    self.view.settings.offset,
                    direction,
                    self.view.dialog_scroll_max + 1,
                );
            }
            _ => return false,
        }
        true
    }

    pub(super) fn settings_guide_key(&mut self, key: KeyPress) {
        if self.view.settings.page == SettingsPage::ServiceGuide && matches!(key.key, Key::Left | Key::Right) {
            let count = self.service_guide_topics().len();
            let index = crate::app::bounded(self.view.settings.guide_topic, if key.key == Key::Left { -1 } else { 1 }, count);
            self.select_settings_guide_topic(index);
            return;
        }
        let delta = match key.key {
            Key::Up => -1,
            Key::Down => 1,
            Key::PageUp => -(self.view.dialog_page_size.max(1) as i64),
            Key::PageDown => self.view.dialog_page_size.max(1) as i64,
            Key::Home => i64::MIN,
            Key::End => i64::MAX,
            Key::Char('c') if key.ctrl => {
                self.copy_help(None);
                return;
            }
            Key::Enter => {
                self.copy_help(None);
                return;
            }
            _ => return,
        };
        self.scroll_online_settings(delta);
    }
}
