use super::super::{
    theme::Palette,
    widgets::{dialog_footer, modal},
};
use crate::{
    app::{App, Hit, Target},
    workspace::{PanelRow, panel_rows},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    selected: &mut usize,
    expanded: &[crate::workspace::Panel],
    area: Rect,
    palette: Palette,
) {
    let rows = panel_rows(expanded);
    let width = area.width.saturating_sub(4).min(88).saturating_sub(2);
    let hints = app.text(
        "Left/Right: expand. Enter: open/toggle.\nSpace: visibility. Up/Down or wheel: select.",
        "←→: раскрыть. Enter: открыть/переключить.\nSpace: видимость. ↑↓ / колесо: выбор.",
    );
    let desired_rows: u16 = rows
        .iter()
        .map(|row| {
            let (label, prefix) = match row {
                PanelRow::Category(panel) => (panel.name(app.settings.language), 6),
                PanelRow::Control(control) => (control.name(app.settings.language), 8),
                PanelRow::Reset => (
                    app.text("Restore default layout", "Вернуть исходную раскладку"),
                    0,
                ),
            };
            super::help::wrapped_height(label, width.saturating_sub(prefix))
        })
        .sum();
    let inner = modal(
        frame,
        area,
        app.text("Blocks and buttons", "Блоки и кнопки").into(),
        (desired_rows + super::help::wrapped_height(hints, width) + 6).min(30),
        palette,
    );
    *selected = (*selected).min(rows.len().saturating_sub(1));
    let hints_height =
        super::help::wrapped_height(hints, inner.width).min(inner.height.saturating_sub(3));
    let capacity = usize::from(inner.height.saturating_sub(hints_height + 4));
    let entries: Vec<_> = rows
        .iter()
        .map(|row| {
            let (prefix, label, muted) = match row {
                PanelRow::Category(panel) => {
                    let indicator = if panel.controls().is_empty() {
                        " "
                    } else if expanded.contains(panel) {
                        "↓"
                    } else {
                        "→"
                    };
                    (
                        format!(
                            "{indicator} [{}] ",
                            if app.settings.workspace.hidden.contains(panel) {
                                ' '
                            } else {
                                'x'
                            },
                        ),
                        panel.name(app.settings.language),
                        false,
                    )
                }
                PanelRow::Control(control) => {
                    let category_hidden = crate::workspace::Panel::ALL
                        .iter()
                        .find(|panel| panel.controls().contains(control))
                        .is_some_and(|panel| app.settings.workspace.hidden.contains(panel));
                    (
                        format!(
                            "    [{}] ",
                            if app.settings.workspace.hidden_controls.contains(control) {
                                ' '
                            } else {
                                'x'
                            },
                        ),
                        control.name(app.settings.language),
                        category_hidden,
                    )
                }
                PanelRow::Reset => (
                    String::new(),
                    app.text("Restore default layout", "Вернуть исходную раскладку"),
                    false,
                ),
            };
            let prefix_width = Span::raw(&prefix).width() as u16;
            let height =
                super::help::wrapped_height(label, inner.width.saturating_sub(prefix_width));
            (prefix, label, muted, prefix_width, height)
        })
        .collect();
    let mut offset = 0;
    let mut used: usize = entries
        .iter()
        .take(*selected + 1)
        .map(|entry| usize::from(entry.4))
        .sum();
    while used > capacity && offset < *selected {
        used -= usize::from(entries[offset].4);
        offset += 1;
    }
    let mut y = inner.y;
    for (index, (prefix, label, muted, prefix_width, height)) in
        entries.iter().enumerate().skip(offset)
    {
        if y.saturating_add(*height) > inner.y.saturating_add(capacity as u16) {
            break;
        }
        let rect = Rect::new(inner.x, y, inner.width, *height);
        y += height;
        let row = &rows[index];
        let style = if index == *selected {
            palette.text().bg(palette.selection)
        } else if app.hovered(rect) {
            palette.text().fg(palette.accent)
        } else if *muted {
            palette.text().fg(palette.muted)
        } else {
            palette.text()
        };
        frame.render_widget(Paragraph::new(prefix.as_str()).style(style), rect);
        frame.render_widget(
            Paragraph::new(*label)
                .style(style)
                .wrap(Wrap { trim: true }),
            Rect::new(
                rect.x + prefix_width,
                rect.y,
                rect.width.saturating_sub(*prefix_width),
                rect.height,
            ),
        );
        app.hits.push(Hit {
            area: rect,
            target: Target::PanelRow(index),
            enabled: true,
        });
        if let PanelRow::Category(panel) = row {
            app.hits.push(Hit {
                area: Rect::new(inner.x + 2, rect.y, 3, 1).intersection(inner),
                target: Target::PanelVisibility(*panel),
                enabled: true,
            });
        }
    }
    frame.render_widget(
        Paragraph::new(hints)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(hints_height + 3),
            inner.width,
            hints_height,
        ),
    );
    dialog_footer(frame, app, inner, palette);
}
