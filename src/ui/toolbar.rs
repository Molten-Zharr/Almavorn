use super::{
    buttons::{button_width, quiet_button},
    theme::Palette,
};
use crate::app::{App, CommandItem, Target};
use ratatui::{Frame, layout::Rect, style::Modifier, widgets::Paragraph};

fn cells(app: &App, width: u16) -> Vec<(Rect, String, Target, bool)> {
    let mut x = 11u16.min(width);
    let mut y = 0;
    app.toolbar_items()
        .into_iter()
        .map(
            |CommandItem {
                 label,
                 target,
                 enabled,
             }| {
                let size = button_width(app, &label, &target).min(width);
                if x > 0 && x.saturating_add(size) > width {
                    x = 0;
                    y += 1;
                }
                let rect = Rect::new(x, y, size, 1);
                x = x.saturating_add(size + 1);
                (rect, label, target, enabled)
            },
        )
        .collect()
}

pub(super) fn height(app: &App, width: u16) -> u16 {
    cells(app, width)
        .last()
        .map_or(1, |(rect, ..)| rect.bottom())
}

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.view.toolbar_areas.clear();
    if let Some(index) = app.view.toolbar_selected
        && !app
            .toolbar_items()
            .get(index)
            .is_some_and(|item| item.enabled)
    {
        app.focus_toolbar(false);
    }
    frame.render_widget(
        Paragraph::new(" ALMAVORN").style(
            palette
                .text()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(area.x, area.y, area.width.min(10), 1),
    );
    for (cell, label, target, enabled) in cells(app, area.width) {
        let rect = Rect::new(area.x + cell.x, area.y + cell.y, cell.width, 1).intersection(area);
        app.view.toolbar_areas.push(rect);
        quiet_button(frame, app, rect, &label, target, enabled, palette);
    }
}
