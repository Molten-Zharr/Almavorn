use super::theme::Palette;
use crate::layout::buttons::{button_caption, displayed_shortcut};
pub(super) use crate::layout::buttons::{button_shortcut, button_width};
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

fn standard(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    let label = button_caption(app, label, &target);
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
    let label = button_caption(app, label, &target);
    let active = match target {
        Target::Mode(mode) => app.settings.mode == mode,
        Target::Action(Action::Panels) => app.view.layout_editing,
        Target::Action(Action::ToggleEdit) => app.selected_playlist_unlocked(),
        Target::FinishEdit => true,
        Target::ToggleLyricsPanel => app.settings.preferences.lyrics.show_panel,
        Target::TogglePlayerSpecialButtons => app.settings.preferences.appearance.show_player_special_buttons,
        Target::Action(Action::PlayWholePlaylist) => app.quick_active(crate::toolbar::QuickButton::Marked),
        Target::Action(Action::SwitchMode) => app.settings.mode == crate::model::Mode::Chaos,
        Target::Action(Action::ToggleDesk) => app.settings.preferences.sorting_desk,
        Target::Action(Action::CycleRepeat) => {
            app.settings.preferences.repeat != crate::model::RepeatMode::Off
        }
        Target::Action(Action::ToggleShuffle) => app.settings.preferences.shuffle,
        Target::Action(Action::PlaybackTuner) => app.tuner_settings() != &Default::default(),
        Target::Action(Action::CycleVisualizer)
        | Target::CommandMenu(crate::app::CommandMenu::Visualizer) => {
            app.settings.preferences.appearance.visualizer != crate::visualizer::VisualizerKind::Off
        }
        Target::TunerReverse => app.tuner_settings().reverse,
        Target::TunerPin => app.tuner_pinned(),
        Target::Action(Action::Equalizer) | Target::EqualizerToggle => {
            app.settings.preferences.equalizer.enabled
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
    if label != "..."
        && let Some(shortcut) = displayed_shortcut(app, &target)
    {
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
    frame.render_widget(Block::default().style(style), area);
    if area.height >= 3 {
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(match palette.corners {
                    Corners::Rounded => BorderType::Rounded,
                    Corners::Square => BorderType::Plain,
                })
                .border_style(style.fg(if enabled && (hovered || active || focused) {
                    palette.accent
                } else {
                    palette.inactive_panel_border
                })),
            area,
        );
    }
    let caption = Rect::new(area.x, area.y + area.height / 2, area.width, 1);
    frame.render_widget(
        Paragraph::new(Line::from(content))
            .style(style)
            .alignment(Alignment::Center),
        caption,
    );
}
