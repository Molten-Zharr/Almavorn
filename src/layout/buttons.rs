use crate::{
    app::{App, Target},
    input::Action,
};
use ratatui::text::Span;

pub(crate) fn button_shortcut(app: &App, target: &Target) -> Option<String> {
    match target {
        Target::Action(action @ (Action::MoveUp | Action::MoveDown))
            if app.view.focus == crate::app::Focus::Playlists =>
        {
            let action = if *action == Action::MoveUp {
                Action::PlaylistUp
            } else {
                Action::PlaylistDown
            };
            app.settings
                .preferences
                .bindings
                .iter()
                .find(|binding| binding.action == action)
                .map(|binding| binding.key.label())
        }
        Target::CommandMenu(crate::app::CommandMenu::Visualizer) => app
            .settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == Action::CycleVisualizer)
            .map(|binding| binding.key.label()),
        Target::Action(action) => app
            .settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == *action)
            .map(|binding| binding.key.label()),
        Target::Mode(mode) if *mode != app.settings.mode => app
            .settings
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == crate::input::Action::SwitchMode)
            .map(|binding| binding.key.label()),
        Target::Submit | Target::BrowserOpen => Some("Enter".into()),
        Target::SearchPlay => Some("Enter".into()),
        Target::Setting(_) => Some("Enter".into()),
        Target::EqualizerToggle => Some("Space".into()),
        Target::EqualizerPresets => Some("Enter".into()),
        Target::EqualizerPreset(index) => Some((index + 1).to_string()),
        Target::EqualizerReset => Some("R".into()),
        Target::TunerReverse => Some("Space".into()),
        Target::TunerReset => Some("R".into()),
        Target::TunerPin => Some("P".into()),
        Target::SettingHelp(_) => Some("F1".into()),
        Target::CloseDialog => Some("Esc".into()),
        Target::BrowserParent => Some("Alt+Up".into()),
        Target::BrowserMarkAll => Some("Ctrl+A".into()),
        Target::BrowserClearMarks => Some("Ctrl+D".into()),
        Target::BrowserAdd => Some("Ctrl+Enter".into()),
        Target::BrowserCurrentFolder => Some("Ctrl+Shift+Enter".into()),
        Target::PlaylistAutoName => Some("Ctrl+F".into()),
        Target::CreateFolderGroups => Some("Ctrl+G".into()),
        Target::FolderAdd => Some("Insert".into()),
        Target::FolderEdit => Some("F3".into()),
        Target::FolderRemove => Some("Delete".into()),
        Target::FolderScan => Some("R".into()),
        Target::ScanSubfolders => Some("Ctrl+Space".into()),
        Target::ComposeMove(direction) => Some(
            if *direction < 0 {
                "Ctrl+Up"
            } else {
                "Ctrl+Down"
            }
            .into(),
        ),
        _ => None,
    }
}

pub(crate) fn button_width(app: &App, label: &str, target: &Target) -> u16 {
    let label = button_caption(label, target);
    // Keep room for the repeat mode marker between the two clicks.
    let label_width = if matches!(target, Target::Action(Action::CycleRepeat)) {
        Span::raw(label).width().max(4)
    } else {
        Span::raw(label).width()
    };
    let shortcut = displayed_shortcut(app, target);
    (label_width
        + shortcut.as_ref().map_or(0, |key| {
            Span::raw(key).width() + if label.is_empty() { 0 } else { 3 }
        })
        + 2)
    .min(usize::from(u16::MAX)) as u16
}

pub(crate) fn displayed_shortcut(app: &App, target: &Target) -> Option<String> {
    if help_button(target) {
        Some("F1".into())
    } else if app.settings.preferences.appearance.show_button_shortcuts {
        button_shortcut(app, target)
    } else {
        None
    }
}

fn help_button(target: &Target) -> bool {
    matches!(
        target,
        Target::Action(crate::input::Action::Help) | Target::SettingHelp(_)
    )
}

pub(crate) fn button_caption<'a>(label: &'a str, target: &Target) -> &'a str {
    if help_button(target) { "" } else { label }
}
