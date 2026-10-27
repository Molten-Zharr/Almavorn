use super::{catalog::Section, keycaps, rich_text, wrapped_height};
use crate::{
    app::{App, Hit, Target},
    help_search::Query,
    ui::{
        theme::Palette,
        widgets::{block, buttons, buttons_height},
    },
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::Span,
    widgets::{Block, Paragraph, Wrap},
};
use std::{
    collections::{HashSet, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
};

pub(super) fn input(frame: &mut Frame, app: &mut App, inner: Rect, palette: Palette) -> Rect {
    if inner.height < 6 || inner.width < 12 {
        return inner;
    }
    let area = Rect::new(inner.x, inner.y, inner.width, 3);
    let border = block(
        app.text("Find an action · Ctrl+F", "Найти действие · Ctrl+F")
            .into(),
        palette,
        true,
    );
    let mut content = border.inner(area);
    frame.render_widget(border, area);
    content.width = content.width.saturating_sub(4);
    let search = &app.view.help_search;
    let characters: Vec<_> = search.query.chars().collect();
    let cursor = search.cursor.min(characters.len());
    let mut start = cursor;
    let mut column = 0;
    while start > 0 {
        let width = Span::raw(characters[start - 1].to_string()).width();
        if column + width >= usize::from(content.width) {
            break;
        }
        column += width;
        start -= 1;
    }
    let empty = search.query.is_empty();
    let text = if empty {
        app.text(
            "Try: make it louder, recover a deleted song",
            "Например: сделать громче, вернуть удаленное",
        )
        .to_owned()
    } else {
        characters[start..].iter().collect()
    };
    let style = if empty {
        palette.text().fg(palette.muted)
    } else if search.selected_all {
        palette.text().bg(palette.selection)
    } else {
        palette.text()
    };
    frame.render_widget(Paragraph::new(text).style(style), content);
    if !content.is_empty() {
        frame.set_cursor_position(Position::new(content.x + column as u16, content.y));
    }
    app.view.hits.push(Hit {
        area,
        target: Target::HelpSearch,
        enabled: true,
    });
    super::super::super::widgets::button(
        frame,
        app,
        Rect::new(area.right().saturating_sub(4), area.y + 1, 3, 1),
        "x",
        Target::HelpClearSearch,
        !empty,
        palette,
    );
    Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 3)
}

fn refresh(app: &mut App, sections: &[Section]) {
    let mut hash = DefaultHasher::new();
    app.view.help_search.query.hash(&mut hash);
    for section in sections {
        section.name.hash(&mut hash);
        for entry in &section.entries {
            entry.search.hash(&mut hash);
            entry.aliases.hash(&mut hash);
            entry.key.map(|key| key.label()).hash(&mut hash);
        }
    }
    let fingerprint = hash.finish();
    if app.view.help_search.fingerprint == Some(fingerprint) {
        return;
    }
    let query = Query::new(&app.view.help_search.query);
    let mut found = Vec::new();
    for (section_index, section) in sections.iter().enumerate() {
        for (entry_index, entry) in section.entries.iter().enumerate() {
            if entry.search.is_empty() {
                continue;
            }
            let key = entry
                .key
                .map(|key| format!("{};{}", key.label(), key.key_labels().join("+")))
                .unwrap_or_default();
            let score = query.score(&entry.search, entry.aliases, &section.search_name, &key);
            if score > 0 {
                let complete = query.covers(&entry.search, entry.aliases, &section.search_name, &key);
                found.push((score, section_index, entry_index, key, complete));
            }
        }
    }
    // Keep partial suggestions only when no explanation covers the whole question.
    if found.iter().any(|result| result.4) {
        found.retain(|result| result.4);
    }
    found.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    let mut seen = HashSet::new();
    app.view.help_search.matches = found
        .into_iter()
        .filter_map(|(_, section, entry, key, _)| {
            seen.insert((key, sections[section].entries[entry].text.as_str()))
                .then_some((section, entry))
        })
        .collect();
    app.view.help_search.selected = app
        .view
        .help_search
        .selected
        .min(app.view.help_search.matches.len().saturating_sub(1));
    app.view.help_search.fingerprint = Some(fingerprint);
}

pub(super) fn results(
    frame: &mut Frame,
    app: &mut App,
    offset: &mut usize,
    inner: Rect,
    sections: &[Section],
    palette: Palette,
) {
    refresh(app, sections);
    let query = Query::new(&app.view.help_search.query);
    let selected = app.view.help_search.selected;
    let controls = vec![
        (
            app.text("Open section · Enter", "К разделу · Enter").into(),
            Target::HelpMatch(selected),
            !app.view.help_search.matches.is_empty(),
        ),
        (
            app.text("Clear · Esc", "Сбросить · Esc").into(),
            Target::HelpClearSearch,
            true,
        ),
        (
            app.text("Close", "Закрыть").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    let footer_height = buttons_height(app, inner.width, &controls).min(inner.height);
    let header_height = inner.height.saturating_sub(footer_height).min(2);
    let body = Rect::new(
        inner.x,
        inner.y + header_height,
        inner.width,
        inner.height.saturating_sub(footer_height + header_height),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}: {} · {}",
            app.text("Found", "Найдено"),
            app.view.help_search.matches.len(),
            app.text(
                "↑↓: choose · Enter: explanation",
                "↑↓: выбрать · Enter: объяснение"
            )
        ))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, header_height),
    );
    if app.view.help_search.matches.is_empty() {
        *offset = 0;
        app.view.help_search.page_size = 1;
        app.view.dialog_page_size = 1;
        app.view.dialog_scroll_max = 0;
        frame.render_widget(Paragraph::new(app.text(
            "No matching action found. Try a shorter description: volume, download folder, move a block, or a shortcut such as Ctrl+B.",
            "Подходящее действие не найдено. Попробуйте короче: громкость, папка загрузки, передвинуть блок или сочетание вроде Ctrl+B."))
            .style(palette.text()).wrap(Wrap { trim: false }), body);
    } else {
        let mut total = 0usize;
        let matches = app.view.help_search.matches.clone();
        let layouts: Vec<_> = matches
            .iter()
            .map(|&(section, entry)| {
                let entry = &sections[section].entries[entry];
                let cap_width = entry.key.map_or(0, keycaps::width);
                let cap_height = entry.key.map_or(0, |key| keycaps::height(key, body.width));
                let stacked = entry.key.is_some() && cap_width.saturating_add(22) > body.width;
                let text_x = if entry.key.is_some() && !stacked {
                    cap_width + 2
                } else {
                    0
                };
                let text_y = 1 + if stacked { cap_height } else { 0 };
                let text_height = wrapped_height(&entry.text, body.width.saturating_sub(text_x));
                let height = (text_y + text_height).max(1 + cap_height) as usize;
                let start = total;
                total += height + 1;
                (start, height, text_x, text_y, text_height, cap_height)
            })
            .collect();
        let (start, height, ..) = layouts[selected];
        if start < *offset {
            *offset = start;
        }
        if start + height > offset.saturating_add(body.height as usize) {
            *offset = (start + height)
                .saturating_sub(body.height as usize)
                .min(start);
        }
        let end = offset.saturating_add(body.height as usize);
        let mut visible = 0;
        for (
            index,
            (
                &(section_index, entry_index),
                &(start, height, text_x, text_y, text_height, cap_height),
            ),
        ) in matches.iter().zip(&layouts).enumerate()
        {
            let top = start.max(*offset);
            let bottom = (start + height).min(end);
            if top >= bottom {
                continue;
            }
            visible += 1;
            let row = Rect::new(
                body.x,
                body.y + (top - *offset) as u16,
                body.width,
                (bottom - top) as u16,
            );
            let style = if index == selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else if app.hovered(row) {
                palette.text().fg(palette.accent)
            } else {
                palette.text()
            };
            frame.render_widget(Block::default().style(style), row);
            let section = &sections[section_index];
            let entry = &section.entries[entry_index];
            let mut text_palette = palette;
            if index == selected {
                text_palette.help_heading = palette.selection_text;
            }
            if start >= *offset {
                frame.render_widget(
                    Paragraph::new(section.name).style(style.fg(palette.help_heading)),
                    Rect::new(body.x, body.y + (start - *offset) as u16, body.width, 1),
                );
            }
            if let Some(key) = entry.key
                && start + 1 >= *offset
                && start + 1 + cap_height as usize <= end
            {
                keycaps::render(
                    frame,
                    app,
                    key,
                    Rect::new(
                        body.x,
                        body.y + (start + 1 - *offset) as u16,
                        body.width,
                        cap_height,
                    ),
                    palette,
                );
            }
            let text_start = start + text_y as usize;
            let text_top = text_start.max(*offset);
            let text_bottom = (text_start + text_height as usize).min(end);
            if text_top < text_bottom {
                frame.render_widget(
                    Paragraph::new(rich_text(&entry.text, text_palette, Some(&query)))
                        .style(style)
                        .wrap(Wrap { trim: true })
                        .scroll(((text_top - text_start) as u16, 0)),
                    Rect::new(
                        body.x + text_x,
                        body.y + (text_top - *offset) as u16,
                        body.width.saturating_sub(text_x),
                        (text_bottom - text_top) as u16,
                    ),
                );
            }
            app.view.hits.push(Hit {
                area: row,
                target: Target::HelpMatch(index),
                enabled: true,
            });
        }
        app.view.help_search.page_size = visible.max(1);
        app.view.dialog_page_size = visible.max(1);
        app.view.dialog_scroll_max = total.saturating_sub(body.height as usize);
    }
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(footer_height),
            inner.width,
            footer_height,
        ),
        controls,
        palette,
    );
}
