use super::theme::Palette;
use crate::app::{App, Target};
use crate::preferences::{BorderWeight, Corners};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, BorderType, Borders, Clear},
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

pub(super) use super::buttons::{button, button_width};

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
        let width = button_width(app, &label, &target).min(area.width);
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
        x = x.saturating_add(width).saturating_add(1);
    }
    y.saturating_sub(area.y) + 1
}

pub(super) fn modal_rect(area: Rect, height: u16) -> Rect {
    let width = area.width.saturating_sub(4).min(88);
    let height = height.min(area.height.saturating_sub(4));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub(super) fn modal(
    frame: &mut Frame,
    area: Rect,
    title: String,
    height: u16,
    palette: Palette,
) -> Rect {
    let rect = modal_rect(area, height);
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
