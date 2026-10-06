use super::super::{keycaps, theme::Palette};
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
    app.view.dialog_scroll_max = paragraph
        .line_count(body.width)
        .saturating_sub(usize::from(body.height))
        .min(usize::from(u16::MAX));
    *offset = (*offset).min(app.view.dialog_scroll_max);
    frame.render_widget(paragraph.scroll((*offset as u16, 0)), body);
}

pub(super) fn help_dialog(
    frame: &mut Frame,
    app: &mut App,
    offset: &mut usize,
    inner: Rect,
    guide: Vec<String>,
    palette: Palette,
) {
    let mut entries: Vec<(Option<KeyPress>, String)> = vec![(
        None,
        app.text("Keyboard shortcuts", "Горячие клавиши").into(),
    )];
    for (key, en, ru) in [
        (
            Key::Escape,
            "Close a menu or dialog, cancel dragging, or finish arranging blocks.",
            "Закрыть меню или диалог, отменить перетаскивание или завершить компоновку.",
        ),
        (
            Key::F(10),
            "Open the focused panel's context menu; right click does the same.",
            "Открыть меню активной панели; также доступно правой кнопкой мыши.",
        ),
        (
            Key::Tab,
            "Focus the next panel or the command bar. On the bar use Left/Right to select, Enter to open, Esc to return to the library.",
            "Перейти к следующей панели или строке команд. В строке: ←→ — выбрать кнопку, Enter — открыть, Esc — вернуться в библиотеку.",
        ),
        (
            Key::Enter,
            "Play or confirm; in the file picker, open a folder or select a file.",
            "Играть или подтвердить; в выборе файлов — открыть папку или отметить файл.",
        ),
        (
            Key::Char(' '),
            "In the file picker, select or unselect a file.",
            "В выборе файлов — поставить или снять отметку файла.",
        ),
        (
            Key::Up,
            "Select above; in playback blocks, increase volume by 1%.",
            "Выбрать выше; в блоках проигрывания — увеличить громкость на 1%.",
        ),
        (
            Key::Down,
            "Select below; in playback blocks, decrease volume by 1%.",
            "Выбрать ниже; в блоках проигрывания — уменьшить громкость на 1%.",
        ),
        (
            Key::PageUp,
            "Scroll one page up.",
            "Прокрутить на страницу вверх.",
        ),
        (
            Key::PageDown,
            "Scroll one page down.",
            "Прокрутить на страницу вниз.",
        ),
    ] {
        entries.push((Some(KeyPress::plain(key)), app.text(en, ru).into()));
    }
    for (ctrl, alt, shift, key, en, ru) in [
        (
            true,
            false,
            false,
            Key::Enter,
            "Add selected files in the file picker.",
            "Добавить выбранные файлы в файловом окне.",
        ),
        (
            false,
            false,
            true,
            Key::Tab,
            "Focus the previous panel or the command bar.",
            "Перейти к предыдущей панели или строке команд.",
        ),
        (
            true,
            true,
            false,
            Key::Char(' '),
            "Collapse or expand the focused block.",
            "Свернуть или развернуть активный блок.",
        ),
        (
            true,
            true,
            false,
            Key::Delete,
            "Hide the focused block; restore it through Blocks.",
            "Скрыть активный блок; вернуть через меню «Блоки».",
        ),
    ] {
        entries.push((
            Some(KeyPress {
                key,
                ctrl,
                alt,
                shift,
            }),
            app.text(en, ru).into(),
        ));
    }
    for binding in &app.settings.bindings {
        entries.push((
            Some(binding.key),
            format!(
                "{}\n{}",
                binding.action.name(app.settings.language),
                binding.action.description(app.settings.language)
            ),
        ));
    }
    for key in [Key::Up, Key::Down, Key::Left, Key::Right] {
        entries.push((
            Some(KeyPress {
                key,
                ctrl: true,
                alt: true,
                shift: false,
            }),
            app.text(
                "Swap the block with a neighbor in this direction.",
                "Обменять блок с соседним в этом направлении.",
            )
            .into(),
        ));
        entries.push((
            Some(KeyPress {
                key,
                ctrl: true,
                alt: false,
                shift: true,
            }),
            app.text(
                "Resize the block in this direction.",
                "Изменить размер блока в этом направлении.",
            )
            .into(),
        ));
    }
    let guide_heading = entries.len();
    entries.push((None, app.text("Guide", "Руководство").into()));
    entries.extend(
        guide
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(|text| (None, text)),
    );
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(3),
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
    app.view.dialog_scroll_max = total_height.saturating_sub(usize::from(body.height));
    *offset = (*offset).min(app.view.dialog_scroll_max);
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
                    .style(palette.text().fg(if index == 0 || index == guide_heading {
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
