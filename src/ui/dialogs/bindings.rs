use super::super::{
    theme::Palette,
    widgets::{button, buttons, modal},
};
use crate::{
    app::{App, Target},
    input::Key,
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};

pub(super) fn binding_dialog(
    frame: &mut Frame,
    app: &mut App,
    index: usize,
    area: Rect,
    palette: Palette,
) {
    let name = app
        .settings
        .bindings
        .get(index)
        .map(|binding| binding.action.name(app.settings.language))
        .unwrap_or("");
    let inner = modal(
        frame,
        area,
        app.text("Set shortcut", "Назначить сочетание").into(),
        27,
        palette,
    );
    let header_height = inner.height.min(4);
    frame.render_widget(
        Paragraph::new(format!(
            "{name}\n{}",
            app.text(
                "Press a shortcut, or choose modifiers and a key below. Esc cancels.",
                "Нажмите сочетание либо выберите модификаторы и клавишу ниже. Esc - отмена."
            )
        ))
        .style(palette.text())
        .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, header_height),
    );
    let modifier_y = inner.y + header_height;
    let footer_y = inner.bottom().saturating_sub(2).max(modifier_y);
    let modifier_height = (footer_y - modifier_y).min(2);
    let modifiers = ["Ctrl", "Alt", "Shift"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            (
                format!(
                    "{} {name}",
                    if app.settings_view.binding_modifiers[index] {
                        "x"
                    } else {
                        " "
                    }
                ),
                Target::BindingModifier(index),
                true,
            )
        })
        .collect();
    buttons(
        frame,
        app,
        Rect::new(inner.x, modifier_y, inner.width, modifier_height),
        modifiers,
        palette,
    );
    let keys: Vec<Key> = Key::shortcut_choices().collect();
    let columns = (inner.width / 8).max(1) as usize;
    let key_y = modifier_y + modifier_height;
    let capacity = columns * usize::from(footer_y.saturating_sub(key_y));
    app.settings_view.binding_offset = app
        .settings_view
        .binding_offset
        .min(keys.len().saturating_sub(capacity.max(1)));
    for (visible, key) in keys
        .iter()
        .copied()
        .skip(app.settings_view.binding_offset)
        .take(capacity)
        .enumerate()
    {
        let label = match key {
            Key::Char(' ') => "Space".into(),
            Key::Char(c) => c.to_uppercase().to_string(),
            Key::F(n) => format!("F{n}"),
            Key::Backspace => "BS".into(),
            Key::Insert => "Ins".into(),
            Key::Delete => "Del".into(),
            Key::PageUp => "PgUp".into(),
            Key::PageDown => "PgDn".into(),
            Key::Up => "↑".into(),
            Key::Down => "↓".into(),
            Key::Left => "←".into(),
            Key::Right => "→".into(),
            other => format!("{other:?}"),
        };
        let x = inner.x + (visible % columns) as u16 * 8;
        button(
            frame,
            app,
            Rect::new(
                x,
                key_y + (visible / columns) as u16,
                (inner.right() - x).min(8),
                1,
            ),
            &label,
            Target::BindingKey(key),
            true,
            palette,
        );
    }
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            footer_y,
            inner.width,
            inner.bottom().saturating_sub(footer_y),
        ),
        vec![
            (
                "↑".into(),
                Target::DialogScroll(-5),
                app.settings_view.binding_offset > 0,
            ),
            (
                "↓".into(),
                Target::DialogScroll(5),
                app.settings_view.binding_offset + capacity < keys.len(),
            ),
            (
                app.text("Cancel", "Отмена").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}
