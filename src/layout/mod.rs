//! Read-only presentation measurements and pure layout calculations.
pub(crate) mod buttons;
pub(crate) mod player;
pub(crate) mod workspace;

use crate::{
    app::{App, CommandMenu, Target},
    input::Action,
    model::RepeatMode,
    preferences::{FontFace, PlaybackTimeline},
    workspace::Panel,
};
use buttons::button_width;
use player::{PlayerControl, PlayerMetrics};
use ratatui::text::Span;
use workspace::WorkspaceMetrics;

pub(crate) fn player_target(action: Action) -> Target {
    if action == Action::CycleVisualizer {
        Target::CommandMenu(CommandMenu::Visualizer)
    } else {
        Target::Action(action)
    }
}

pub(crate) fn player_metrics(app: &App) -> PlayerMetrics {
    let controls = [
        (Action::Previous, "‹"),
        (Action::TogglePlay, if app.paused() { "[▶]" } else { "[‖]" }),
        (Action::Stop, "[■]"),
        (Action::Next, "›"),
        (
            Action::CycleRepeat,
            match (
                app.settings.preferences.repeat,
                app.settings.preferences.appearance.font,
            ) {
                (RepeatMode::Off, FontFace::DejaVuMono) => "[↻]",
                (RepeatMode::Playlist, FontFace::DejaVuMono) => "[↻∞]",
                (RepeatMode::Track, FontFace::DejaVuMono) => "[↻1]",
                (RepeatMode::Off, _) => "[⟳]",
                (RepeatMode::Playlist, _) => "[⟳∞]",
                (RepeatMode::Track, _) => "[⟳1]",
            },
        ),
        (
            Action::ToggleShuffle,
            if app.settings.preferences.appearance.font == FontFace::DejaVuMono {
                "[⇄]"
            } else {
                "[↔]"
            },
        ),
        (Action::Equalizer, "[EQ]"),
        (Action::PlaybackTuner, "[FX]"),
        (Action::CycleVisualizer, "[VIS↓]"),
    ]
    .into_iter()
    .filter(|(action, _)| app.control_visible(Panel::Player, *action))
    .filter(|(action, _)| app.settings.preferences.appearance.show_player_special_buttons
        || !matches!(action, Action::Equalizer | Action::PlaybackTuner | Action::CycleVisualizer))
    .map(|(action, label)| PlayerControl {
        action,
        label,
        width: button_width(app, label, &player_target(action))
            .saturating_add(app.settings.preferences.appearance.player_button_size.padding()),
    })
    .collect();
    PlayerMetrics {
        controls,
        checked_loop: app.marked_playback_count().is_some(),
        waveform: app.playback_active()
            && app.settings.preferences.appearance.playback_timeline == PlaybackTimeline::Waveform,
    }
}

pub(crate) fn title_reserved(app: &App, panel: Panel) -> u16 {
    if app.view.layout_editing {
        13
    } else if panel == Panel::Playlists
        && app.control_visible(Panel::PlaylistActions, Action::NewPlaylist)
    {
        button_width(app, "+", &Target::Action(Action::NewPlaylist)).saturating_add(6)
    } else {
        9
    }
}

pub(crate) fn workspace_metrics(app: &App) -> WorkspaceMetrics {
    WorkspaceMetrics {
        playlist_placement: app.settings.preferences.playlist_placement,
        player_placement: app.settings.preferences.player_placement,
        hidden: Panel::CONTENT
            .into_iter()
            .filter(|panel| !app.panel_visible(*panel))
            .collect(),
        collapsed: app.settings.preferences.workspace.collapsed.clone(),
        playlist_min_width: (title_reserved(app, Panel::Playlists)
            + Span::raw(Panel::Playlists.name(app.settings.preferences.language)).width() as u16)
            .max(18),
        player: player_metrics(app),
    }
}
