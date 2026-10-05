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

fn wrapped_height(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let mut total = 0usize;
    for line in text.lines() {
        let (mut rows, mut used) = (1usize, 0usize);
        for word in line.split_whitespace() {
            let length = Span::raw(word).width();
            if used > 0 && used + 1 + length > width {
                rows += 1;
                used = 0;
            }
            if length > width {
                rows += (length - 1) / width;
                used = (length - 1) % width + 1;
            } else {
                used += usize::from(used > 0) + length;
            }
        }
        total += rows;
    }
    total.max(1).min(usize::from(u16::MAX)) as u16
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
            "Close the window or cancel dragging.",
            "Закрыть окно или отменить перетаскивание.",
        ),
        (
            Key::Tab,
            "Focus the next block.",
            "Перейти к следующему блоку.",
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
            "Focus the previous block.",
            "Перейти к предыдущему блоку.",
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
    entries.push((None, app.text("Guide", "Руководство").into()));
    entries.extend(
        guide
            .into_iter()
            .filter(|text| !text.is_empty())
            .map(|text| (None, text)),
    );
    *offset = (*offset).min(entries.len().saturating_sub(1));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(3),
    );
    let mut y = body.y;
    for (key, text) in entries.iter().skip(*offset) {
        let cap_width = key.map_or(0, keycaps::width);
        let cap_height = key.map_or(0, |key| keycaps::height(key, body.width));
        let stacked = key.is_some() && cap_width.saturating_add(22) > body.width;
        let text_x = if key.is_some() && !stacked {
            body.x + cap_width + 2
        } else {
            body.x
        };
        let text_y = y + if stacked { cap_height } else { 0 };
        let width = body.right().saturating_sub(text_x);
        let paragraph = Paragraph::new(text.as_str())
            .style(palette.text())
            .wrap(Wrap { trim: true });
        let text_height = wrapped_height(text, width);
        let height = if stacked {
            text_height + cap_height
        } else {
            text_height.max(cap_height)
        };
        if y.saturating_add(height) > body.bottom() {
            break;
        }
        if let Some(key) = key {
            keycaps::render(
                frame,
                app,
                *key,
                Rect::new(body.x, y, body.width, cap_height),
                palette,
            );
        }
        frame.render_widget(paragraph, Rect::new(text_x, text_y, width, text_height));
        y = y.saturating_add(height).saturating_add(1);
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
    app.hits.push(Hit {
        area: Rect::new(footer.x, footer.y, close_width.min(footer.width), 2),
        target: Target::CloseDialog,
        enabled: true,
    });
    for (index, key, delta) in [(0, Key::Up, -1), (1, Key::Down, 1)] {
        let rect =
            Rect::new(footer.x + close_width + 2 + index * 4, footer.y, 3, 2).intersection(footer);
        keycaps::render(frame, app, KeyPress::plain(key), rect, palette);
        app.hits.push(Hit {
            area: rect,
            target: Target::DialogScroll(delta),
            enabled: true,
        });
    }
}
