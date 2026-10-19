//! Commands used by a compositor to control an already running player.
use crate::input::{Action, Key, KeyPress};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
mod transport;
#[cfg(unix)]
pub use transport::{Incoming, Server, send};

pub const COMMANDS: &[(Action, &str)] = &[
    (Action::TogglePlay, "play-pause"),
    (Action::Stop, "stop"),
    (Action::Next, "next"),
    (Action::Previous, "previous"),
    (Action::VolumeUp, "volume-up"),
    (Action::VolumeDown, "volume-down"),
    (Action::SeekForward, "seek-forward"),
    (Action::SeekBackward, "seek-backward"),
    (Action::CycleRepeat, "repeat"),
    (Action::ToggleShuffle, "shuffle"),
    (Action::Equalizer, "equalizer"),
    (Action::PlaybackTuner, "playback-tuner"),
    (Action::CycleVisualizer, "visualizer"),
    (Action::SwitchMode, "switch-mode"),
    (Action::ToggleEdit, "edit"),
    (Action::ToggleDesk, "sorting-desk"),
    (Action::Help, "help"),
    (Action::Settings, "settings"),
    (Action::Statistics, "statistics"),
    (Action::AddFiles, "add-files"),
    (Action::AddFolder, "add-folder"),
    (Action::AddFolderPlaylists, "folder-playlists"),
    (Action::AddNew, "add-new"),
    (Action::PlaylistFolders, "playlist-folders"),
    (Action::NewPlaylist, "new-playlist"),
    (Action::CopyPlaylist, "copy-playlist"),
    (Action::ComposePlaylists, "compose-playlists"),
    (Action::Rename, "rename"),
    (Action::Delete, "delete"),
    (Action::MoveUp, "move-up"),
    (Action::MoveDown, "move-down"),
    (Action::PlaylistUp, "playlist-up"),
    (Action::PlaylistDown, "playlist-down"),
    (Action::Transfer, "copy-to-playlist"),
    (Action::Search, "search"),
    (Action::Filter, "filter"),
    (Action::Sort, "sort"),
    (Action::Undo, "undo"),
    (Action::Redo, "redo"),
    (Action::Metadata, "track-info"),
    (Action::Mark, "mark"),
    (Action::PlayMarked, "play-marked"),
    (Action::PlayWholePlaylist, "play-playlist"),
    (Action::PlaylistPanel, "playlist-panel"),
    (Action::PlayerPanel, "player-panel"),
    (Action::Panels, "panels"),
    (Action::Quit, "quit"),
];

pub fn command_name(action: Action) -> &'static str {
    COMMANDS
        .iter()
        .find(|(value, _)| *value == action)
        .map_or("", |(_, name)| *name)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Action(Action),
    Play,
    Pause,
    Volume(u8),
    Status,
    NiriBindings,
}

impl Request {
    pub fn parse(command: &str) -> Result<Self> {
        Ok(match command {
            "play" => Self::Play,
            "pause" => Self::Pause,
            "status" => Self::Status,
            "niri-bindings" => Self::NiriBindings,
            name => Self::Action(COMMANDS.iter().find(|(_, value)| *value == name)
                .map(|(action, _)| *action)
                .ok_or_else(|| anyhow::anyhow!("Unknown command / Неизвестная команда: {name}. --commands: список команд."))?),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GlobalShortcut {
    pub action: Action,
    pub key: String,
    pub enabled: bool,
}

/// Keep configuration as XKB names so Super and media keys need no key capture.
pub fn normalize_shortcut(value: &str) -> Result<String> {
    let pieces: Vec<_> = value.trim().split('+').collect();
    ensure!(
        !value.is_empty() && value.len() <= 96 && pieces.len() <= 5,
        "Введите сочетание, например Super+Space или XF86AudioPlay."
    );
    let (key, modifiers) = pieces.split_last().expect("split always yields an item");
    ensure!(
        !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "Используйте имя клавиши XKB: Space, Return, Left, F8, XF86AudioPlay."
    );
    let mut chosen = [false; 4];
    for modifier in modifiers {
        let index = match modifier.to_ascii_lowercase().as_str() {
            "super" | "win" | "mod" => 0,
            "ctrl" | "control" => 1,
            "alt" => 2,
            "shift" => 3,
            _ => anyhow::bail!("Модификаторы: Super, Ctrl, Alt, Shift."),
        };
        ensure!(!chosen[index], "Модификатор указан дважды.");
        chosen[index] = true;
    }
    let mut parts: Vec<String> = ["Super", "Ctrl", "Alt", "Shift"]
        .into_iter()
        .zip(chosen)
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| name.into())
        .collect();
    parts.push(if key.len() == 1 {
        key.to_ascii_uppercase()
    } else {
        (*key).into()
    });
    Ok(parts.join("+"))
}

pub fn suggested_shortcut(key: KeyPress) -> String {
    let name = match key.key {
        Key::Char(' ') => "Space".into(),
        Key::Char('+') | Key::Char('=') => "equal".into(),
        Key::Char('-') => "minus".into(),
        Key::Char('/') => "slash".into(),
        Key::Char(',') => "comma".into(),
        Key::Char('.') => "period".into(),
        Key::Char(';') => "semicolon".into(),
        Key::Char('[') => "bracketleft".into(),
        Key::Char(']') => "bracketright".into(),
        Key::Char('\\') => "backslash".into(),
        Key::Char('\'') => "apostrophe".into(),
        Key::Char('`') => "grave".into(),
        Key::Char(c) => c.to_uppercase().to_string(),
        Key::Enter => "Return".into(),
        Key::Escape => "Escape".into(),
        Key::Backspace => "BackSpace".into(),
        Key::PageUp => "Page_Up".into(),
        Key::PageDown => "Page_Down".into(),
        Key::F(n) => format!("F{n}"),
        other => format!("{other:?}"),
    };
    let mut parts = vec!["Super".to_owned()];
    for (enabled, modifier) in [
        (key.ctrl, "Ctrl"),
        (key.alt, "Alt"),
        (key.shift || key.key == Key::Char('+'), "Shift"),
    ] {
        if enabled {
            parts.push(modifier.into());
        }
    }
    parts.push(name);
    parts.join("+")
}

#[derive(Serialize, Deserialize)]
pub(super) struct Response {
    pub ok: bool,
    pub message: String,
}
