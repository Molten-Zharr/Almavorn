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
        Target::CloseDialog => Some("Esc".into()),
        Target::BrowserParent => Some("Backspace".into()),
        Target::BrowserMarkAll => Some("Ctrl+A".into()),
        Target::BrowserAdd => Some("Ctrl+Enter".into()),
        _ => None,
    }
}

pub(super) fn button_width(app: &App, label: &str, target: &Target) -> u16 {
    let shortcut = button_shortcut(app, target);
    (Span::raw(label).width()
        + shortcut
            .as_ref()
            .map_or(0, |key| Span::raw(key).width() + 3)
        + 2)
    .min(usize::from(u16::MAX)) as u16
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
    let shortcut = button_shortcut(app, &target);
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
        content.push(Span::styled(" · ", style.fg(palette.muted)));
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
    let active = match target {
        Target::Mode(mode) => app.settings.mode == mode,
        Target::Action(Action::Panels) => app.view.layout_editing,
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
    frame.render_widget(
        Paragraph::new(label)
            .style(style)
            .alignment(Alignment::Center),
        area,
    );
}
