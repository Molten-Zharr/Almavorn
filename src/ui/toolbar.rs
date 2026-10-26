use super::{
    buttons::quiet_button,
    theme::Palette,
};
use crate::app::{App, Target};
use ratatui::{Frame, layout::Rect, style::Modifier, widgets::Paragraph};

fn cells(app: &App) -> Vec<(Rect, String, Target, bool)> {
    let (placed, more, _) = app.quick_layout();
    let areas = placed.into_iter().map(|(_, area)| area).chain(more);
    areas.zip(app.toolbar_items()).map(|(area, item)| (area, item.label, item.target, item.enabled)).collect()
}

pub(super) fn height(app: &App, _width: u16) -> u16 {
    app.quick_layout().2
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
    for (cell, label, target, enabled) in cells(app) {
        let rect = Rect::new(area.x + cell.x, area.y + cell.y, cell.width, cell.height).intersection(area);
        app.view.toolbar_areas.push(rect);
        quiet_button(frame, app, rect, &label, target, enabled, palette);
    }
}
