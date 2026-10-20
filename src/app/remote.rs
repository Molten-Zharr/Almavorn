use super::{App, Dialog, SettingsEdit, SettingsFocus, SettingsPage, TextPurpose};
use crate::{
    input::Action,
    remote::{GlobalShortcut, Request, command_name, normalize_shortcut, suggested_shortcut},
};
use anyhow::{Context, Result, ensure};
use std::path::Path;

impl App {
    pub fn new_with_commands(directory: &Path) -> Result<Self> {
        #[cfg(unix)]
        let server = crate::remote::Server::bind(directory)?;
        let mut app = Self::new(directory)?;
        #[cfg(unix)]
        {
            app.remote = Some(server);
        }
        #[cfg(target_os = "linux")]
        app.start_mpris();
        Ok(app)
    }

    #[cfg(unix)]
    pub(super) fn tick_remote(&mut self) {
        let Some(server) = self.remote.take() else {
            return;
        };
        // Datagram reads never wait for a client, and each frame has bounded work.
        for _ in 0..16 {
            match server.receive() {
                Ok(Some(incoming)) => {
                    let result = self.remote_request(&incoming.request);
                    server.respond(&incoming, result);
                    if self.view.quit {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.failure(error);
                    break;
                }
            }
        }
        self.remote = Some(server);
    }

    fn remote_request(&mut self, request: &Request) -> Result<String> {
        match request {
            Request::Status => {
                return Ok(serde_json::to_string_pretty(&serde_json::json!({
                    "playing": self.playback_active() && !self.paused(),
                    "paused": (self.playback_active() || self.preparing_playback()) && self.paused(),
                    "preparing": self.preparing_playback(),
                    "volume": (self.settings.preferences.volume * 100.0).round() as u8,
                    "position_ms": self.position_ms(),
                    "track": self.playback.current.as_ref().map(|track| &track.title),
                    "mode": self.settings.mode.key(),
                }))?);
            }
            Request::NiriBindings => return self.niri_bindings(),
            Request::Volume(value) => {
                ensure!(*value <= 100, "Громкость: от 0 до 100");
                self.set_volume(f32::from(*value) / 100.0)?;
            }
            Request::Play => {
                if !(self.playback_active() || self.preparing_playback()) || self.paused() {
                    self.remote_action(Action::TogglePlay)?;
                }
            }
            Request::Pause => {
                if (self.playback_active() || self.preparing_playback()) && !self.paused() {
                    self.remote_action(Action::TogglePlay)?;
                }
            }
            Request::Action(action) => self.remote_action(*action)?,
        }
        Ok(String::new())
    }

    fn remote_action(&mut self, action: Action) -> Result<()> {
        ensure!(
            self.allowed(action),
            "{}",
            self.text(
                "Action unavailable in the current state",
                "Действие недоступно в текущем состоянии плеера"
            )
        );
        self.action(action)
    }

    pub(crate) fn niri_bindings(&self) -> Result<String> {
        let program = &self.command_program;
        let directory = self
            .store
            .path
            .parent()
            .context("Library directory unavailable")?
            .canonicalize()?;
        let quote = |path: &Path| -> Result<String> {
            let text = path
                .to_str()
                .context("niri configuration requires UTF-8 paths")?;
            ensure!(
                !text.chars().any(char::is_control),
                "niri configuration cannot use control characters in paths"
            );
            Ok(serde_json::to_string(text)?)
        };
        let program = quote(program)?;
        let directory = quote(&directory)?;
        let mut output = String::from("binds {\n");
        for binding in &self.settings.preferences.global_shortcuts {
            if !binding.enabled {
                continue;
            }
            let key = normalize_shortcut(&binding.key)?;
            let command = command_name(binding.action);
            ensure!(!command.is_empty(), "Unknown global shortcut action");
            let repeat = matches!(
                binding.action,
                Action::VolumeUp | Action::VolumeDown | Action::SeekForward | Action::SeekBackward
            );
            output.push_str(&format!("    {key} repeat={repeat} {{ spawn {program} \"--data-dir\" {directory} \"--command\" \"{command}\"; }}\n"));
        }
        output.push_str("}\n");
        Ok(output)
    }

    pub(super) fn edit_global_shortcut(&mut self, index: usize) {
        let Some(binding) = self.settings.preferences.bindings.get(index) else {
            return;
        };
        let text = self
            .settings
            .preferences
            .global_shortcuts
            .iter()
            .find(|global| global.action == binding.action)
            .map_or_else(
                || suggested_shortcut(binding.key),
                |global| global.key.clone(),
            );
        self.text_dialog(
            TextPurpose::Settings(SettingsEdit::GlobalShortcut(binding.action)),
            text,
        );
        if let Some(Dialog::Text(dialog)) = &mut self.view.dialog {
            dialog.keyboard = crate::model::Language::English;
        }
    }

    pub(super) fn set_global_shortcut(&mut self, action: Action, text: &str) -> Result<()> {
        let key = normalize_shortcut(text)?;
        ensure!(
            !self
                .settings
                .preferences
                .global_shortcuts
                .iter()
                .any(|other| other.enabled && other.action != action && other.key == key),
            "Это глобальное сочетание уже назначено другому действию."
        );
        if let Some(binding) = self
            .settings
            .preferences
            .global_shortcuts
            .iter_mut()
            .find(|global| global.action == action)
        {
            binding.key = key;
            binding.enabled = true;
        } else {
            self.settings
                .preferences
                .global_shortcuts
                .push(GlobalShortcut {
                    action,
                    key,
                    enabled: true,
                });
        }
        Ok(())
    }

    pub(super) fn toggle_global_shortcut(&mut self, index: usize) -> Result<()> {
        let Some(binding) = self.settings.preferences.bindings.get(index) else {
            return Ok(());
        };
        let action = binding.action;
        if let Some(global) = self
            .settings
            .preferences
            .global_shortcuts
            .iter()
            .find(|global| global.action == action)
        {
            if global.enabled {
                self.settings
                    .preferences
                    .global_shortcuts
                    .iter_mut()
                    .find(|global| global.action == action)
                    .expect("found")
                    .enabled = false;
            } else {
                self.set_global_shortcut(action, &global.key.clone())?;
            }
            self.save_settings()?;
        } else {
            self.edit_global_shortcut(index);
        }
        Ok(())
    }

    pub(super) fn show_niri_bindings(&mut self) -> Result<()> {
        let configuration = self.niri_bindings()?;
        let empty = !self
            .settings
            .preferences
            .global_shortcuts
            .iter()
            .any(|binding| binding.enabled);
        let explanation = if empty {
            self.text(
                "No niri keys are enabled. Open an action, set a niri key and check 'Export to niri'. MPRIS works independently of this export.",
                "Клавиши niri пока не заданы. Откройте действие, задайте клавишу для niri и включите отметку \"В конфиг niri\". MPRIS работает независимо от этого экспорта."
            )
        } else {
            self.text(
                "Add this block to niri's config.kdl. Regenerate it after changing keys or the active profile.",
                "Добавьте этот блок в config.kdl niri. После изменения клавиш или профиля обновите блок."
            )
        };
        self.view.dialog = Some(Dialog::SettingsHelp {
            title: self
                .text("Manual configuration · niri", "Ручная настройка · niri")
                .into(),
            description: format!(
                "{explanation}\n\n{}\n{}",
                if empty { "" } else { &configuration },
                self.text(
                    "Terminal export: --niri-bindings",
                    "Вывод в терминал: --niri-bindings"
                )
            ),
            offset: 0,
        });
        self.view.settings.focus = SettingsFocus::Parameters;
        self.view.settings.page = SettingsPage::Shortcuts;
        Ok(())
    }

    pub(crate) fn system_media_status(&self) -> String {
        #[cfg(target_os = "linux")]
        {
            use crate::mpris::Status;
            return match self
                .mpris
                .as_ref()
                .map(|bridge| bridge.status.borrow().clone())
            {
                Some(Status::Connected(_)) => self.text("Connected", "Подключено"),
                Some(Status::Connecting) => self.text("Connecting…", "Подключение…"),
                Some(Status::Unavailable(_)) => {
                    self.text("D-Bus unavailable", "Нет соединения D-Bus")
                }
                None => self.text("Not connected", "Не подключено"),
            }
            .into();
        }
        #[cfg(not(target_os = "linux"))]
        self.text("Linux only", "Только Linux").into()
    }

    pub(super) fn show_system_media(&mut self) {
        let mut details = self.system_media_status();
        let mut player = "almavorn".to_owned();
        #[cfg(target_os = "linux")]
        if let Some(bridge) = &self.mpris {
            match bridge.status.borrow().clone() {
                crate::mpris::Status::Connected(name) => {
                    player = name
                        .trim_start_matches("org.mpris.MediaPlayer2.")
                        .to_owned();
                    details.push_str(&format!("\n{name}"));
                }
                crate::mpris::Status::Unavailable(error) => details.push_str(&format!(
                    "\n{error}\n{}",
                    self.text(
                        "The player will retry automatically.",
                        "Плеер повторит подключение автоматически."
                    )
                )),
                _ => {}
            }
        }
        self.view.dialog = Some(Dialog::CopyableHelp {
            title: self.text("System media controls · MPRIS", "Системное управление · MPRIS").into(),
            description: format!("{details}\n\n{}\n\n{}\nplayerctl --player={player} play-pause\nplayerctl --player={player} next\nplayerctl --player={player} volume 0.05+\n\n{}\nalmavorn --play-pause\nalmavorn --next", self.text(
                "MPRIS connects this player to your desktop over the session D-Bus. Media keys and system panels can control playback without focusing Almavorn. Desktop support and the selected active player determine where media keys go.",
                "MPRIS связывает плеер с окружением через сеансовую шину D-Bus. Медиаклавиши и системные панели могут управлять воспроизведением без фокуса Almavorn. Обработка медиаклавиш и выбор активного плеера зависят от окружения."
            ), self.text("If playerctl is installed:", "Если установлен playerctl:"), self.text(
                "Custom global shortcuts belong in your desktop's keyboard settings. MPRIS does not register arbitrary keys. Existing Almavorn commands also work:",
                "Произвольные глобальные сочетания назначаются в настройках клавиатуры окружения. MPRIS сам их не регистрирует. Также работают команды Almavorn:"
            )),
            offset: 0,
        });
    }
}
