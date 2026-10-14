use super::{
    buttons::button_shortcut,
    theme::Palette,
    widgets::{block, visible_offset},
};
use crate::{
    app::{App, CommandMenu, Hit, Target},
    input::Action,
};
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
    let block = block(
        menu.title(app.settings.preferences.language).into(),
        palette,
        false,
    )
    .border_style(palette.text().fg(palette.separator));
    let inner = block.inner(outer);
    app.view.overlay_area = outer;
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
        frame.render_widget(Paragraph::new("").style(style), rect);
        let shortcut = button_shortcut(app, &item.target);
        let key_width = shortcut
            .as_ref()
            .map_or(0, |key| Span::raw(key).width() as u16)
            .min(rect.width.saturating_sub(1));
        let label_width =
            rect.width
                .saturating_sub(if shortcut.is_some() { key_width + 2 } else { 0 });
        let label = if matches!(&item.target, Target::Action(Action::PlayMarked))
            && Span::raw(&item.label).width() + 1 > usize::from(label_width)
        {
            format!(
                "[{}] {}",
                if item.label.starts_with("[x]") {
                    'x'
                } else {
                    ' '
                },
                app.text("Checked tracks only", "Только отмеченные")
            )
        } else {
            item.label.clone()
        };
        frame.render_widget(
            Paragraph::new(format!(" {label}")).style(style),
            Rect::new(rect.x, rect.y, label_width, 1),
        );
        if let Some(key) = shortcut {
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
