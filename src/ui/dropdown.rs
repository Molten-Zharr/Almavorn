use super::{theme::Palette, widgets::block};
use crate::app::{App, CommandItem, Hit, Target};
use ratatui::{Frame, layout::Rect, style::Color, text::Span, widgets::{Clear, Paragraph}};

pub struct DropdownRow {
    pub source: Rect,
    pub top: f32,
    pub height: f32,
    pub background: Color,
    pub target: Target,
}

pub struct Dropdown {
    pub area: Rect,
    pub rows: Vec<DropdownRow>,
    pub background: Color,
}

pub(super) fn row_height(app: &App, item: &CommandItem) -> f32 {
    if matches!(item.target, Target::MenuSection) {
        if item.label.is_empty() && app.view.graphical_keycaps { 0.5 } else { 1.0 }
    } else if app.view.graphical_keycaps { 1.5 } else { 2.0 }
}

pub(super) fn height(app: &App, items: &[CommandItem], footer: bool) -> u16 {
    items.iter().map(|item| row_height(app, item)).sum::<f32>().ceil() as u16 + 2 + u16::from(footer)
}

pub(super) fn render(frame: &mut Frame, app: &mut App, outer: Rect, title: String,
    items: &[CommandItem], selected: usize, footer: Option<&str>, palette: Palette,
    row_target: fn(usize, &CommandItem) -> Target) {
    frame.render_widget(Clear, outer);
    let border = block(title, palette, false).border_style(palette.text().fg(palette.separator));
    let inner = border.inner(outer);
    frame.render_widget(border, outer);
    let body = Rect { height: inner.height.saturating_sub(u16::from(footer.is_some())), ..inner };
    let mut offset = 0;
    let mut used: f32 = items.iter().take(selected + 1).map(|item| row_height(app, item)).sum();
    while used > f32::from(body.height) && offset < selected {
        used -= row_height(app, &items[offset]);
        offset += 1;
    }
    let mut dropdown = Dropdown { area: body, rows: Vec::new(), background: palette.background };
    let mut top = f32::from(body.y);
    let mut end = offset;
    for (index, item) in items.iter().enumerate().skip(offset) {
        let height = row_height(app, item);
        if top + height > f32::from(body.bottom()) { break; }
        let section = matches!(item.target, Target::MenuSection);
        let source = Rect::new(body.x, top.floor() as u16, body.width, u16::from(!item.label.is_empty()));
        let style = if section { palette.text().fg(palette.accent) }
            else if !item.enabled { palette.text().fg(palette.muted) }
            else if index == selected { palette.text().bg(palette.selection).fg(palette.selection_text) }
            else { palette.text() };
        let key = if section { None } else { crate::layout::buttons::displayed_shortcut(app, &item.target) };
        let key_width = key.as_ref().map_or(0, |key| Span::raw(key).width() as u16).min(body.width.saturating_sub(3));
        let label_width = body.width.saturating_sub(if key.is_some() { key_width + 3 } else { 0 });
        frame.render_widget(Paragraph::new("").style(style), source);
        frame.render_widget(Paragraph::new(format!(" {}", item.label)).style(style), Rect { width: label_width, ..source });
        if let Some(key) = key {
            frame.render_widget(Paragraph::new(key).style(style.fg(palette.muted)), Rect::new(body.right().saturating_sub(key_width + 1), source.y, key_width, 1));
        }
        let target = if section || !item.enabled { Target::MenuSection } else { row_target(index, item) };
        if !app.view.graphical_keycaps {
            let row = Rect { height: height as u16, ..source };
            // Give the terminal version the same generous hit area as its visible row.
            if row.height > 1 { frame.render_widget(Paragraph::new("").style(style), Rect::new(row.x, row.y + 1, row.width, row.height - 1)); }
            app.view.hits.push(Hit { area: row, target, enabled: true });
        } else {
            app.view.hits.push(Hit { area: source, target: target.clone(), enabled: true });
            dropdown.rows.push(DropdownRow { source, top, height, background: style.bg.unwrap_or(palette.background), target });
        }
        top += height;
        end = index + 1;
    }
    if app.view.graphical_keycaps { app.view.dropdown = Some(dropdown); }
    if outer.width > 1 && outer.height > 3 {
        for (shown, symbol, y) in [(offset > 0, "↑", outer.y + 1), (end < items.len(), "↓", outer.bottom() - 2)] {
            if shown { frame.render_widget(Paragraph::new(symbol).style(palette.text().fg(palette.accent)), Rect::new(outer.right() - 1, y, 1, 1)); }
        }
    }
    if let Some(text) = footer {
        frame.render_widget(Paragraph::new(text).style(palette.text().fg(palette.muted)), Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1));
    }
}
