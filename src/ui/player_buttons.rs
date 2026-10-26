use super::theme::Palette;
use crate::{
    app::{App, Hit},
    input::Action,
    layout::{buttons::displayed_shortcut, player_target},
    model::RepeatMode,
    visualizer::VisualizerKind,
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Color,
    widgets::{Block, Paragraph},
};

/// The same icon geometry is used by the graphical and terminal interfaces.
pub struct PlayerButton {
    pub area: Rect,
    pub shortcut_area: Rect,
    pub shortcut: Option<String>,
    /// Convex polygons on a 24 by 24 canvas.
    pub polygons: Vec<Vec<[f32; 2]>>,
    pub foreground: Color,
    pub background: Color,
    pub border: Color,
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    bounds: Rect,
    action: Action,
    palette: Palette,
) {
    if area.is_empty() {
        return;
    }
    let target = player_target(action);
    let enabled = app.allowed(action);
    let active = match action {
        Action::CycleRepeat => app.settings.preferences.repeat != RepeatMode::Off,
        Action::ToggleShuffle => app.settings.preferences.shuffle,
        Action::Equalizer => app.settings.preferences.equalizer.enabled,
        Action::PlaybackTuner => app.tuner_settings() != &Default::default(),
        Action::CycleVisualizer => {
            app.settings.preferences.appearance.visualizer != VisualizerKind::Off
        }
        _ => false,
    };
    let highlighted = enabled && (app.hovered(area) || active);
    let button = PlayerButton {
        area,
        shortcut_area: Rect::new(area.x, area.bottom(), area.width, 1).intersection(bounds),
        shortcut: displayed_shortcut(app, &target),
        polygons: polygons(app, action),
        foreground: if !enabled {
            palette.muted
        } else if highlighted {
            palette.accent
        } else {
            palette.button_text
        },
        background: if highlighted {
            palette.selection
        } else {
            palette.background
        },
        border: if !enabled {
            palette.muted
        } else if highlighted {
            palette.accent
        } else {
            palette.inactive_panel_border
        },
    };
    frame.render_widget(Block::default().style(palette.text()), area);
    if !app.view.graphical_keycaps {
        paint_terminal(frame, &button);
        if let Some(shortcut) = &button.shortcut {
            frame.render_widget(
                Paragraph::new(shortcut.as_str())
                    .alignment(Alignment::Center)
                    .style(palette.text().fg(button.foreground)),
                button.shortcut_area,
            );
        }
    }
    app.view.player_buttons.push(button);
    app.view.hits.push(Hit {
        area,
        target,
        enabled,
    });
}

fn polygons(app: &App, action: Action) -> Vec<Vec<[f32; 2]>> {
    let mut shapes = Vec::new();
    let rectangle =
        |x: f32, y: f32, w: f32, h: f32| vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
    let line = |a: [f32; 2], b: [f32; 2]| {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy).max(f32::EPSILON);
        let (x, y) = (-dy / length, dx / length);
        vec![
            [a[0] + x, a[1] + y],
            [b[0] + x, b[1] + y],
            [b[0] - x, b[1] - y],
            [a[0] - x, a[1] - y],
        ]
    };
    match action {
        Action::TogglePlay if !app.paused() => {
            shapes.push(rectangle(4.0, 2.0, 5.0, 20.0));
            shapes.push(rectangle(15.0, 2.0, 5.0, 20.0));
        }
        Action::TogglePlay => shapes.push(vec![[5.0, 2.0], [22.0, 12.0], [5.0, 22.0]]),
        Action::Stop => shapes.push(rectangle(3.0, 3.0, 18.0, 18.0)),
        Action::Previous | Action::Next => {
            shapes.push(rectangle(19.0, 2.0, 3.0, 20.0));
            shapes.push(vec![[2.0, 2.0], [17.0, 12.0], [2.0, 22.0]]);
            if action == Action::Previous {
                for point in shapes.iter_mut().flatten() {
                    point[0] = 24.0 - point[0];
                }
            }
        }
        Action::CycleRepeat => {
            for (a, b) in [
                ([3.0, 10.0], [3.0, 5.0]),
                ([3.0, 5.0], [19.0, 5.0]),
                ([21.0, 14.0], [21.0, 19.0]),
                ([21.0, 19.0], [5.0, 19.0]),
            ] {
                shapes.push(line(a, b));
            }
            shapes.push(vec![[17.0, 1.0], [23.0, 5.0], [17.0, 9.0]]);
            shapes.push(vec![[7.0, 15.0], [1.0, 19.0], [7.0, 23.0]]);
            if app.settings.preferences.repeat == RepeatMode::Track {
                shapes.push(line([10.0, 10.0], [13.0, 8.0]));
                shapes.push(line([13.0, 8.0], [13.0, 16.0]));
            }
        }
        Action::ToggleShuffle => {
            for (a, b) in [
                ([2.0, 5.0], [6.0, 5.0]),
                ([6.0, 5.0], [18.0, 19.0]),
                ([2.0, 19.0], [6.0, 19.0]),
                ([6.0, 19.0], [10.0, 14.0]),
                ([14.0, 10.0], [18.0, 5.0]),
            ] {
                shapes.push(line(a, b));
            }
            for y in [5.0, 19.0] {
                shapes.push(line([18.0, y], [21.0, y]));
                shapes.push(vec![[18.0, y - 4.0], [24.0, y], [18.0, y + 4.0]]);
            }
        }
        Action::Equalizer => {
            for (x, y) in [(4.0, 7.0), (12.0, 16.0), (20.0, 10.0)] {
                shapes.push(rectangle(x - 1.0, 2.0, 2.0, 20.0));
                shapes.push(rectangle(x - 3.0, y - 2.0, 6.0, 4.0));
            }
        }
        Action::PlaybackTuner => {
            shapes.push(vec![[2.0, 19.0], [15.0, 6.0], [18.0, 9.0], [5.0, 22.0]]);
            for (x, y) in [(5.0, 5.0), (19.0, 3.0), (21.0, 17.0)] {
                shapes.push(rectangle(x - 1.0, y - 3.0, 2.0, 6.0));
                shapes.push(rectangle(x - 3.0, y - 1.0, 6.0, 2.0));
            }
        }
        Action::CycleVisualizer => {
            for (x, height) in [(1.0, 9.0), (7.0, 20.0), (13.0, 14.0), (19.0, 18.0)] {
                shapes.push(rectangle(x, 22.0 - height, 4.0, height));
            }
        }
        _ => {}
    }
    shapes
}

fn paint_terminal(frame: &mut Frame, button: &PlayerButton) {
    let area = button.area;
    let width = usize::from(area.width);
    let height = usize::from(area.height) * 2;
    let sample = |x: usize, y: usize| {
        if x == 0 || x + 1 == width || y == 0 || y + 1 == height {
            return button.border;
        }
        let point = [
            ((x as f32 + 0.5) / width as f32 - 0.12) / 0.76 * 24.0,
            ((y as f32 + 0.5) / height as f32 - 0.12) / 0.76 * 24.0,
        ];
        let inside = button.polygons.iter().any(|polygon| {
            let mut positive = false;
            let mut negative = false;
            for index in 0..polygon.len() {
                let a = polygon[index];
                let b = polygon[(index + 1) % polygon.len()];
                let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
                positive |= cross > 0.0;
                negative |= cross < 0.0;
            }
            !(positive && negative)
        });
        if inside {
            button.foreground
        } else {
            button.background
        }
    };
    for y in 0..area.height {
        for x in 0..area.width {
            if let Some(cell) = frame.buffer_mut().cell_mut((area.x + x, area.y + y)) {
                cell.set_symbol("▀")
                    .set_fg(sample(usize::from(x), usize::from(y) * 2))
                    .set_bg(sample(usize::from(x), usize::from(y) * 2 + 1));
            }
        }
    }
}
