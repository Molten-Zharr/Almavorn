use super::theme::Palette;
use crate::app::{App, Hit, Target};
use crate::preferences::{BorderWeight, Corners};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub(super) fn block(title: String, palette: Palette, active: bool) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(if palette.borders == BorderWeight::None {
            Borders::NONE
        } else {
            Borders::ALL
        })
        .border_type(match (palette.borders, palette.corners) {
            (BorderWeight::Double, _) => BorderType::Double,
            (BorderWeight::Thick, _) => BorderType::Thick,
            (_, Corners::Rounded) => BorderType::Rounded,
            _ => BorderType::Plain,
        })
        .border_style(Style::default().fg(if active {
            palette.accent
        } else {
            palette.inactive_panel_border
        }))
        .style(palette.text())
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
    if area.width == 0 || area.height == 0 {
        return;
    }
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
    frame.render_widget(Paragraph::new(format!("[{label}] ")).style(style), area);
    app.hits.push(Hit {
        area,
        target,
        enabled,
    });
}

pub(super) fn buttons(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    entries: Vec<(String, Target, bool)>,
    palette: Palette,
) -> u16 {
    let mut x = area.x;
    let mut y = area.y;
    for (label, target, enabled) in entries {
        let width = (label.chars().count() as u16 + 3).min(area.width);
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        button(
            frame,
            app,
            Rect::new(x, y, width, 1),
            &label,
            target,
            enabled,
            palette,
        );
        x += width;
    }
    y.saturating_sub(area.y) + 1
}

pub(super) fn modal(
    frame: &mut Frame,
    area: Rect,
    title: String,
    height: u16,
    palette: Palette,
) -> Rect {
    let width = area.width.saturating_sub(4).min(88);
    let height = height.min(area.height.saturating_sub(4));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let outer = block(title, palette, true);
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    inner
}

pub(super) fn dialog_footer(frame: &mut Frame, app: &mut App, inner: Rect, palette: Palette) {
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            ("↑".into(), Target::DialogScroll(-5), true),
            ("↓".into(), Target::DialogScroll(5), true),
            (
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}
pub(super) fn visible_offset(
    previous: usize,
    selected: usize,
    capacity: usize,
    length: usize,
) -> usize {
    let capacity = capacity.max(1);
    let mut offset = previous.min(length.saturating_sub(capacity));
    if selected < offset {
        offset = selected;
    } else if selected >= offset + capacity {
        offset = selected + 1 - capacity;
    }
    offset
}
pub(super) fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
