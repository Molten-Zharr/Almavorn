use super::{
    buttons::button_shortcut,
    theme::Palette,
    widgets::{block, visible_offset},
};
use crate::app::{App, CommandMenu, Hit, Target};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    menu: CommandMenu,
    selected: &mut usize,
    anchor: Position,
    palette: Palette,
) {
    let items = app.command_items(menu);
    let area = frame.area();
    let width = items
        .iter()
        .map(|item| {
            Span::raw(&item.label).width()
                + button_shortcut(app, &item.target).map_or(0, |key| Span::raw(key).width() + 3)
                + 4
        })
        .max()
        .unwrap_or(24)
        .min(usize::from(area.width.saturating_sub(2))) as u16;
    let height = (items.len() as u16 + 3).min(area.height.saturating_sub(2));
    let outer = Rect::new(
        anchor.x.min(area.right().saturating_sub(width)),
        anchor
            .y
            .saturating_add(1)
            .min(area.bottom().saturating_sub(height)),
        width,
        height,
    );
    let block = block(menu.title(app.settings.language).into(), palette, false)
        .border_style(palette.text().fg(palette.separator));
    let inner = block.inner(outer);
    app.view.hits.push(Hit {
        area,
        target: Target::CloseDialog,
        enabled: true,
    });
    frame.render_widget(Clear, outer);
    frame.render_widget(block, outer);
    *selected = App::command_selection(&items, *selected, 0);
    let capacity = inner.height.saturating_sub(1) as usize;
    let offset = visible_offset(0, *selected, capacity, items.len());
    for (index, item) in items.iter().enumerate().skip(offset).take(capacity) {
        let rect = Rect::new(inner.x, inner.y + (index - offset) as u16, inner.width, 1);
        let style = if !item.enabled {
            palette.text().fg(palette.muted)
        } else if index == *selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(format!(" {}", item.label)).style(style),
            rect,
        );
        if let Some(key) = button_shortcut(app, &item.target) {
            let key_width = Span::raw(&key).width() as u16;
            frame.render_widget(
                Paragraph::new(key).style(style.fg(palette.muted)),
                Rect::new(
                    rect.right().saturating_sub(key_width + 1),
                    rect.y,
                    key_width,
                    1,
                ),
            );
        }
        app.view.hits.push(Hit {
            area: rect,
            target: Target::CommandRow(index),
            enabled: item.enabled,
        });
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            app.text("↑↓ · Enter · Esc", "↑↓ · Enter · Esc"),
            palette.text().fg(palette.muted),
        )])),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}
