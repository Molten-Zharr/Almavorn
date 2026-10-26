use super::super::{keycaps, theme::Palette};
mod catalog;
mod find;
use crate::{
    app::{App, Hit, Target},
    input::{Key, KeyPress},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::Span,
    widgets::{Paragraph, Wrap},
};

pub(super) fn wrapped_height(text: &str, width: u16) -> u16 {
    Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .line_count(width.max(1))
        .min(usize::from(u16::MAX)) as u16
}

pub(super) fn scroll_text(
    frame: &mut Frame,
    app: &mut App,
    offset: &mut usize,
    body: Rect,
    text: String,
    palette: Palette,
) {
    let paragraph = Paragraph::new(text)
        .style(palette.text())
        .wrap(Wrap { trim: false });
    app.view.dialog_page_size = usize::from(body.height);
    app.view.dialog_scroll_max = paragraph
        .line_count(body.width)
        .saturating_sub(usize::from(body.height))
        .min(usize::from(u16::MAX));
    *offset = (*offset).min(app.view.dialog_scroll_max);
    frame.render_widget(paragraph.scroll((*offset as u16, 0)), body);
}

pub(in crate::ui) fn copyable_text(
    frame: &mut Frame,
    app: &mut App,
    offset: &mut usize,
    inner: Rect,
    description: &str,
    palette: Palette,
) {
    let footer_height = inner.height.min(3);
    let hint_height = inner.height.saturating_sub(footer_height).min(2);
    let body = Rect::new(inner.x, inner.y, inner.width,
        inner.height.saturating_sub(footer_height + hint_height));
    let lines: Vec<_> = description.lines().map(|line| {
        let height = Paragraph::new(line).wrap(Wrap { trim: false })
            .line_count(body.width.max(1)).max(1);
        (line, height)
    }).collect();
    let total: usize = lines.iter().map(|(_, height)| height).sum();
    app.view.dialog_page_size = usize::from(body.height);
    app.view.dialog_scroll_max = total.saturating_sub(body.height as usize);
    *offset = (*offset).min(app.view.dialog_scroll_max);
    let mut start = 0usize;
    for (index, (line, height)) in lines.into_iter().enumerate() {
        let end = start.saturating_add(height);
        let visible_start = start.max(*offset);
        let visible_end = end.min(offset.saturating_add(body.height as usize));
        if visible_start < visible_end {
            let row = Rect::new(body.x, body.y + (visible_start - *offset) as u16,
                body.width, (visible_end - visible_start) as u16);
            let copyable = !line.trim().is_empty();
            let style = if copyable && app.hovered(row) {
                palette.text().fg(palette.accent).add_modifier(Modifier::UNDERLINED)
            } else {
                palette.text()
            };
            frame.render_widget(Paragraph::new(line).style(style).wrap(Wrap { trim: false })
                .scroll(((visible_start - start).min(u16::MAX as usize) as u16, 0)), row);
            if copyable {
                app.view.hits.push(Hit { area: row, target: Target::CopyHelp(Some(index)), enabled: true });
            }
        }
        start = end;
    }
    frame.render_widget(Paragraph::new(app.text(
        "Click a line to copy it. Ctrl+C: copy all text.",
        "Нажмите строку для копирования. Ctrl+C: весь текст.",
    )).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false }),
        Rect::new(inner.x, body.bottom(), inner.width, hint_height));
    super::super::widgets::buttons(frame, app,
        Rect::new(inner.x, inner.bottom().saturating_sub(footer_height), inner.width, footer_height),
        vec![
            ("↑".into(), Target::DialogScroll(-5), *offset > 0),
            ("↓".into(), Target::DialogScroll(5), *offset < app.view.dialog_scroll_max),
            (app.text("Copy text", "Копировать текст").into(), Target::CopyHelp(None), true),
            (app.text("Close", "Закрыть").into(), Target::CloseDialog, true),
        ], palette);
}

pub(super) fn help_dialog(
    frame: &mut Frame,
    app: &mut App,
    offset: &mut usize,
    inner: Rect,
    guide: Vec<String>,
    palette: Palette,
) {
    let sections = catalog::sections(app, guide);
    let inner = find::input(frame, app, inner, palette);
    if app.view.help_search.active() {
        find::results(frame, app, offset, inner, &sections, palette);
        return;
    }
    let names: Vec<_> = sections.iter().map(|section| section.name).collect();
    let mut entries = Vec::new();
    let mut headings = Vec::new();
    let mut identities = Vec::new();
    for (section_index, section) in sections.into_iter().enumerate() {
        headings.push(entries.len());
        entries.push((None, section.name.to_owned()));
        identities.push(None);
        for (entry_index, entry) in section.entries.into_iter().enumerate() {
            entries.push((entry.key, entry.text));
            identities.push(Some((section_index, entry_index)));
        }
    }
    let header_height = inner.height.saturating_sub(3).min(2);
    let sidebar_width = if inner.width >= 74 { 22 } else { 0 };
    let body = Rect::new(
        inner.x + sidebar_width,
        inner.y + header_height,
        inner.width.saturating_sub(sidebar_width),
        inner.height.saturating_sub(3 + header_height),
    );
    // Offset is measured in rendered lines, including wrapped descriptions and keycaps.
    let layouts: Vec<_> = entries
        .iter()
        .map(|(key, text)| {
            let cap_width = key.map_or(0, keycaps::width);
            let cap_height = key.map_or(0, |key| keycaps::height(key, body.width));
            let stacked = key.is_some() && cap_width.saturating_add(22) > body.width;
            let text_x = if key.is_some() && !stacked {
                cap_width + 2
            } else {
                0
            };
            let text_y = if stacked { cap_height } else { 0 };
            let text_height = wrapped_height(text, body.width.saturating_sub(text_x));
            let height = (text_y + text_height).max(cap_height);
            (text_x, text_y, text_height, cap_height, height)
        })
        .collect();
    let total_height = layouts
        .iter()
        .map(|(.., height)| usize::from(*height) + 1)
        .sum::<usize>()
        .saturating_sub(1);
    app.view.dialog_page_size = usize::from(body.height);
    app.view.dialog_scroll_max = total_height.saturating_sub(usize::from(body.height));
    if let Some(target) = app.view.help_search.pending_entry.take()
        && let Some(index) = identities.iter().position(|identity| *identity == Some(target))
    {
        *offset = layouts[..index].iter().map(|(.., height)| usize::from(*height) + 1).sum();
    }
    *offset = (*offset).min(app.view.dialog_scroll_max);
    app.view.help_sections.clear();
    let mut section_start = 0usize;
    for (index, layout) in layouts.iter().enumerate() {
        if headings.contains(&index) { app.view.help_sections.push(section_start); }
        section_start += usize::from(layout.4) + 1;
    }
    let current = if *offset == app.view.dialog_scroll_max && *offset > 0 { names.len() - 1 }
        else { app.view.help_sections.partition_point(|start| *start <= *offset).saturating_sub(1) };
    if sidebar_width > 0 {
        frame.render_widget(Paragraph::new(app.text("Categories", "Категории"))
            .style(palette.text().fg(palette.help_heading).add_modifier(Modifier::BOLD)),
            Rect::new(inner.x, inner.y, sidebar_width.saturating_sub(1), 1));
        let first = super::super::widgets::visible_offset(0, current, body.height as usize, names.len());
        for (index, name) in names.iter().enumerate().skip(first).take(body.height as usize) {
            let rect = Rect::new(inner.x, body.y + (index - first) as u16, sidebar_width - 2, 1);
            let style = if index == current { palette.text().bg(palette.selection).fg(palette.selection_text) }
                else if app.hovered(rect) { palette.text().fg(palette.accent) } else { palette.text() };
            frame.render_widget(Paragraph::new(*name).style(style), rect);
            app.view.hits.push(Hit { area: rect, target: Target::HelpSection(index), enabled: true });
        }
        frame.render_widget(Paragraph::new(app.text(
            "Type to search · Tab: section · PgUp/PgDn: page",
            "Введите запрос для поиска · Tab: раздел · PgUp/PgDn: страница"))
            .style(palette.text().fg(palette.muted)).wrap(Wrap { trim: true }),
            Rect::new(body.x, inner.y, body.width, header_height));
    } else if header_height > 0 {
        super::super::widgets::button(frame, app, Rect::new(inner.x, inner.y, inner.width.min(3), 1),
            "‹", Target::HelpSection(current.saturating_sub(1)), current > 0, palette);
        frame.render_widget(Paragraph::new(format!("{}/{} · {}", current + 1, names.len(), names[current]))
            .style(palette.text().fg(palette.help_heading)),
            Rect::new(inner.x + inner.width.min(4), inner.y, inner.width.saturating_sub(8), 1));
        super::super::widgets::button(frame, app,
            Rect::new(inner.right().saturating_sub(3).max(inner.x), inner.y, inner.width.min(3), 1),
            "›", Target::HelpSection((current + 1).min(names.len() - 1)), current + 1 < names.len(), palette);
        if header_height > 1 {
            frame.render_widget(Paragraph::new(app.text("Tab: category · PgUp/PgDn: page", "Tab: раздел · PgUp/PgDn: страница"))
                .style(palette.text().fg(palette.muted)), Rect::new(inner.x, inner.y + 1, inner.width, 1));
        }
    }
    let mut cursor = 0;
    let visible_end = *offset + usize::from(body.height);
    for (index, ((key, text), &(text_x, text_y, text_height, cap_height, height))) in
        entries.iter().zip(&layouts).enumerate()
    {
        let start = cursor;
        cursor += usize::from(height) + 1;
        if start >= visible_end {
            break;
        }
        if start + usize::from(height) <= *offset {
            continue;
        }
        if let Some(key) = key
            && start >= *offset
            && start + usize::from(cap_height) <= visible_end
        {
            keycaps::render(
                frame,
                app,
                *key,
                Rect::new(
                    body.x,
                    body.y + (start - *offset) as u16,
                    body.width,
                    cap_height,
                ),
                palette,
            );
        }
        let text_start = start + usize::from(text_y);
        let top = text_start.max(*offset);
        let bottom = (text_start + usize::from(text_height)).min(visible_end);
        if top < bottom {
            frame.render_widget(
                Paragraph::new(text.as_str())
                    .style(palette.text().fg(if headings.contains(&index) {
                        palette.help_heading
                    } else {
                        palette.text
                    }))
                    .wrap(Wrap { trim: true })
                    .scroll(((top - text_start) as u16, 0)),
                Rect::new(
                    body.x + text_x,
                    body.y + (top - *offset) as u16,
                    body.width.saturating_sub(text_x),
                    (bottom - top) as u16,
                ),
            );
        }
    }
    let footer = Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2);
    keycaps::render(frame, app, KeyPress::plain(Key::Escape), footer, palette);
    let close = app.text("Close", "Закрыть");
    let close_width =
        keycaps::width(KeyPress::plain(Key::Escape)) + Span::raw(close).width() as u16 + 2;
    frame.render_widget(
        Paragraph::new(close).style(palette.text().add_modifier(Modifier::BOLD)),
        Rect::new(footer.x + 7, footer.y, footer.width.saturating_sub(7), 1),
    );
    app.view.hits.push(Hit {
        area: Rect::new(footer.x, footer.y, close_width.min(footer.width), 2),
        target: Target::CloseDialog,
        enabled: true,
    });
    for (index, key, delta) in [(0, Key::Up, -1), (1, Key::Down, 1)] {
        let rect =
            Rect::new(footer.x + close_width + 2 + index * 4, footer.y, 3, 2).intersection(footer);
        keycaps::render(
            frame,
            app,
            KeyPress::plain(key),
            Rect::new(rect.x, rect.y, rect.width.saturating_add(1), rect.height)
                .intersection(footer),
            palette,
        );
        app.view.hits.push(Hit {
            area: rect,
            target: Target::DialogScroll(delta),
            enabled: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Language;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn wrapped_guide_tail_is_visible_on_the_last_body_row_and_cannot_scroll_past_it() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = App::new(directory.path()).unwrap();
        for language in [Language::Russian, Language::English] {
            app.settings.preferences.language = language;
            let marker = app.text("LAST LINE", "ПОСЛЕДНЯЯ");
            for (width, height) in [(86, 36), (64, 21), (26, 9)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let inner = Rect::new(0, 0, width, height);
                let palette = Palette::new(&app);
                // The final paragraph itself exceeds the viewport and must be
                // partially rendered, including its last line after wrapping.
                let guide = vec![
                    "Earlier guide section".repeat(4),
                    format!("{}\n{marker}", "wrapped words ".repeat(200)),
                ];
                let mut offset = usize::MAX;
                terminal
                    .draw(|frame| {
                        help_dialog(frame, &mut app, &mut offset, inner, guide.clone(), palette);
                    })
                    .unwrap();
                assert!(offset > 0);
                let maximum = offset;
                // The last three rows are reserved for the dialog footer.
                let body_last_y = inner.bottom() - 4;
                let last_line: String = (inner.x..inner.right())
                    .map(|x| terminal.backend().buffer()[(x, body_last_y)].symbol())
                    .collect();
                assert_eq!(last_line.trim(), marker, "{width}x{height}, {language:?}");

                offset += 1;
                terminal
                    .draw(|frame| {
                        help_dialog(frame, &mut app, &mut offset, inner, guide.clone(), palette);
                    })
                    .unwrap();
                assert_eq!(offset, maximum);
                let last_line: String = (inner.x..inner.right())
                    .map(|x| terminal.backend().buffer()[(x, body_last_y)].symbol())
                    .collect();
                assert_eq!(last_line.trim(), marker, "{width}x{height}, {language:?}");
            }
        }
    }
}
