use super::theme::Palette;
use crate::{
    app::{App, Keycap},
    input::KeyPress,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::Span,
    widgets::{Block, Paragraph},
};

pub(super) fn width(key: KeyPress) -> u16 {
    let labels = key.key_labels();
    labels
        .iter()
        .map(|label| Span::raw(label).width() as u16 + 2)
        .sum::<u16>()
        + 3 * labels.len().saturating_sub(1) as u16
}

fn layout(key: KeyPress, width: u16) -> Vec<(String, Rect, bool)> {
    let mut x = 0;
    let mut y = 0;
    key.key_labels()
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            let separator = index > 0;
            let gap = if separator { 3 } else { 0 };
            let cap_width = Span::raw(&label).width() as u16 + 2;
            if x > 0 && x + gap + cap_width > width {
                x = 0;
                y += 2;
            }
            x += gap;
            let rect = Rect::new(x, y, cap_width, 2);
            x += cap_width;
            (label, rect, separator)
        })
        .collect()
}

pub(super) fn height(key: KeyPress, width: u16) -> u16 {
    layout(key, width)
        .last()
        .map_or(0, |(_, rect, _)| rect.bottom())
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    key: KeyPress,
    area: Rect,
    palette: Palette,
) {
    for (label, relative, separator) in layout(key, area.width) {
        let x = area.x + relative.x;
        let y = area.y + relative.y;
        if separator {
            frame.render_widget(
                Paragraph::new(" + ").style(palette.text().fg(palette.muted)),
                Rect::new(x - 3, y, 3, 1).intersection(area),
            );
        }
        let width = relative.width;
        let rect = Rect::new(x, y, width, 2).intersection(area);
        if rect.width != width || rect.height != 2 {
            break;
        }
        if app.view.graphical_keycaps {
            // The GUI paints its own key faces. Keep the backing raster blank:
            // font backgrounds can extend beyond a cell during incremental redraws.
            frame.render_widget(Block::default().style(palette.text()), rect);
        } else {
            frame.render_widget(
                Paragraph::new(format!("[{label}]")).style(
                    palette
                        .text()
                        .bg(palette.selection)
                        .fg(palette.button_text)
                        .add_modifier(Modifier::BOLD),
                ),
                Rect::new(x, y, width, 1),
            );
            frame.render_widget(
                Paragraph::new(" ".repeat(usize::from(width))).style(palette.text()),
                Rect::new(x, y + 1, width, 1),
            );
        }
        app.view.keycaps.push(Keycap { area: rect, label });
    }
}
