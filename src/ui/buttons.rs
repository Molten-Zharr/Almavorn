use super::theme::Palette;
use crate::{
    app::{App, Hit, Target},
    input::Action,
    preferences::Corners,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

#[derive(Clone, Copy)]
pub(super) enum ButtonKind {
    Standard,
    Quiet,
}

pub(super) struct Button<'a> {
    pub label: &'a str,
    pub target: Target,
    pub enabled: bool,
    pub kind: ButtonKind,
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    button: Button<'_>,
    palette: Palette,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Button {
        label,
        target,
        enabled,
        kind,
    } = button;
    match kind {
        ButtonKind::Standard => standard(frame, app, area, label, target.clone(), enabled, palette),
        ButtonKind::Quiet => quiet(frame, app, area, label, target.clone(), enabled, palette),
    }
    app.view.hits.push(Hit {
        area,
        target,
        enabled,
    });
}

pub(super) fn button(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    render(
        frame,
        app,
        area,
        Button {
            label,
            target,
            enabled,
            kind: ButtonKind::Standard,
        },
        palette,
    );
}

pub(super) fn quiet_button(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    render(
        frame,
        app,
        area,
        Button {
            label,
            target,
            enabled,
            kind: ButtonKind::Quiet,
        },
        palette,
    );
}

pub(super) fn button_shortcut(app: &App, target: &Target) -> Option<String> {
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
                .bindings
                .iter()
                .find(|binding| binding.action == action)
                .map(|binding| binding.key.label())
        }
        Target::Action(action) => app
            .settings
            .bindings
            .iter()
            .find(|binding| binding.action == *action)
            .map(|binding| binding.key.label()),
        Target::Mode(mode) if *mode != app.settings.mode => app
            .settings
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
        Target::SettingHelp(_) => Some("F1".into()),
        Target::CloseDialog => Some("Esc".into()),
        Target::BrowserParent => Some("Backspace".into()),
        Target::BrowserMarkAll => Some("Ctrl+A".into()),
        Target::BrowserAdd => Some("Ctrl+Enter".into()),
        Target::PlaylistAutoName => Some("Ctrl+F".into()),
        Target::CreateFolderGroups => Some("Ctrl+G".into()),
        Target::FolderAdd => Some("Insert".into()),
        Target::FolderEdit => Some("F3".into()),
        Target::FolderRemove => Some("Delete".into()),
        Target::FolderScan => Some("R".into()),
        Target::ScanSubfolders => Some("Space".into()),
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

pub(super) fn button_width(app: &App, label: &str, target: &Target) -> u16 {
    let label = button_caption(label, target);
    let shortcut = displayed_shortcut(app, target);
    (Span::raw(label).width()
        + shortcut.as_ref().map_or(0, |key| {
            Span::raw(key).width() + if label.is_empty() { 0 } else { 3 }
        })
        + 2)
    .min(usize::from(u16::MAX)) as u16
}

fn displayed_shortcut(app: &App, target: &Target) -> Option<String> {
    if help_button(target) {
        Some("F1".into())
    } else if app.settings.appearance.show_button_shortcuts {
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

fn button_caption<'a>(label: &'a str, target: &Target) -> &'a str {
    if help_button(target) { "" } else { label }
}

fn standard(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    let label = button_caption(label, &target);
    let mut style = if enabled {
        palette
            .text()
            .fg(palette.button_text)
            .add_modifier(Modifier::BOLD)
    } else {
        palette.text().fg(palette.muted)
    };
    if enabled && app.hovered(area) {
        style = style.bg(palette.selection);
    }
    if enabled && matches!(target, Target::Submit) {
        style = style.bg(palette.confirm_background);
    }
    let shortcut = displayed_shortcut(app, &target);
    let mut content = Vec::new();
    if let Some(shortcut) = &shortcut {
        content.push(Span::styled(
            shortcut.clone(),
            style.fg(if enabled {
                palette.accent
            } else {
                palette.muted
            }),
        ));
        if !label.is_empty() {
            content.push(Span::styled(" · ", style.fg(palette.muted)));
        }
    }
    content.push(Span::styled(label, style));
    let border_style = style.bg(palette.background).fg(if !enabled {
        palette.muted
    } else if app.hovered(area) || matches!(target, Target::Action(crate::input::Action::Panels)) {
        palette.accent
    } else {
        palette.inactive_panel_border
    });
    if area.height >= 3 {
        let outer = Block::default()
            .borders(Borders::ALL)
            .style(palette.text())
            .border_type(match palette.corners {
                Corners::Rounded => BorderType::Rounded,
                Corners::Square => BorderType::Plain,
            })
            .border_style(border_style);
        let inner = outer.inner(area);
        frame.render_widget(outer, area);
        frame.render_widget(
            Paragraph::new(Line::from(content))
                .alignment(Alignment::Center)
                .style(style),
            inner,
        );
    } else {
        frame.render_widget(Block::default().style(palette.text()), area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y,
            area.width.saturating_sub(2),
            area.height,
        );
        frame.render_widget(
            Paragraph::new(Line::from(content))
                .style(style)
                .alignment(Alignment::Center),
            inner,
        );
        for x in [area.x, area.right().saturating_sub(1)] {
            frame.render_widget(
                Paragraph::new("│").style(border_style),
                Rect::new(x, area.y, 1, area.height),
            );
        }
    }
}

pub(super) fn quiet_width(label: &str) -> u16 {
    (Span::raw(label).width() + 2).min(usize::from(u16::MAX)) as u16
}

fn quiet(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    let label = button_caption(label, &target);
    let active = match target {
        Target::Mode(mode) => app.settings.mode == mode,
        Target::Action(Action::Panels) => app.view.layout_editing,
        Target::Action(Action::ToggleEdit) => app.view.editing,
        Target::Action(Action::ToggleDesk) => app.settings.sorting_desk,
        Target::Action(Action::CycleRepeat) => app.settings.repeat != crate::model::RepeatMode::Off,
        Target::Action(Action::ToggleShuffle) => app.settings.shuffle,
        Target::Action(Action::Equalizer) | Target::EqualizerToggle => {
            app.settings.equalizer.enabled
        }
        Target::Action(Action::Filter) => app.view.filter_editing || !app.library.query.is_empty(),
        _ => false,
    };
    let hovered = app.hovered(area);
    let mut style = palette.text().fg(if enabled {
        palette.button_text
    } else {
        palette.muted
    });
    if enabled && (hovered || active) {
        style = style.bg(palette.selection).fg(palette.accent);
    }
    let focused = !app
        .view
        .dialog
        .as_ref()
        .is_some_and(crate::app::Dialog::is_settings)
        && app
            .view
            .toolbar_selected
            .and_then(|index| app.view.toolbar_areas.get(index))
            .is_some_and(|rect| *rect == area);
    if enabled && focused {
        style = style.bg(palette.sidebar_selection).fg(palette.background);
    }
    let mut content = Vec::new();
    if let Some(shortcut) = displayed_shortcut(app, &target) {
        content.push(Span::styled(
            format!("[{shortcut}]"),
            style.fg(if !enabled {
                palette.muted
            } else if focused {
                palette.background
            } else {
                palette.accent
            }),
        ));
        if !label.is_empty() {
            content.push(Span::styled(" ", style));
        }
    }
    content.push(Span::styled(label, style));
    frame.render_widget(
        Paragraph::new(Line::from(content))
            .style(style)
            .alignment(Alignment::Center),
        area,
    );
}
