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
        .preferences
        .bindings
        .get(index)
        .map(|binding| binding.action.name(app.settings.preferences.language))
        .unwrap_or("");
    let inner = modal(
        frame,
        app,
        area,
        app.text("Set shortcut", "Назначить сочетание").into(),
        31,
        palette,
    );
    let header_height = inner.height.min(4);
    frame.render_widget(
        Paragraph::new(format!(
            "{name}\n{}",
            app.text(
                "Press a local shortcut. Tab: niri setup. Esc: cancel.",
                "Нажмите локальное сочетание. Tab: настройка niri. Esc: отмена."
            )
        ))
        .style(palette.text())
        .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, header_height),
    );
    let error_y = inner.y + header_height;
    let footer_y = inner.bottom().saturating_sub(2).max(error_y);
    let error_height = if let Some(error) = &app.view.settings.binding_error {
        let height = (Paragraph::new(error.as_str())
            .wrap(Wrap { trim: false })
            .line_count(inner.width.max(1)) as u16)
            .min(footer_y.saturating_sub(error_y));
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, error_y, inner.width, height),
        );
        height
    } else {
        0
    };
    let modifier_y = error_y + error_height;
    let modifier_height = (footer_y - modifier_y).min(2);
    let modifiers = ["Ctrl", "Alt", "Shift"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            (
                format!(
                    "{} {name}",
                    if app.view.settings.binding_modifiers[index] {
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
    let global_y = modifier_y + modifier_height;
    let global = app
        .settings
        .preferences
        .bindings
        .get(index)
        .and_then(|binding| {
            app.settings
                .preferences
                .global_shortcuts
                .iter()
                .find(|global| global.action == binding.action)
        });
    let enabled = global.is_some_and(|global| global.enabled);
    let key = global
        .map(|global| global.key.clone())
        .unwrap_or_else(|| app.text("Not set", "Не задана").into());
    let global_height = footer_y.saturating_sub(global_y).min(2);
    buttons(
        frame,
        app,
        Rect::new(inner.x, global_y, inner.width, global_height),
        vec![
            (
                format!(
                    "[{}] {}",
                    if enabled { 'x' } else { ' ' },
                    app.text("Export to niri", "В конфиг niri")
                ),
                if app.view.settings.binding_global_focus == Some(0) {
                    Target::Submit
                } else {
                    Target::BindingGlobal(index)
                },
                true,
            ),
            (
                app.text("Set niri key…", "Клавиша для niri…").into(),
                if app.view.settings.binding_global_focus == Some(1) {
                    Target::Submit
                } else {
                    Target::BindingGlobalEdit(index)
                },
                true,
            ),
        ],
        palette,
    );
    let description_y = global_y + global_height;
    let description = format!(
        "{}: {key}\nalmavorn --command {}\n{}",
        app.text("niri key", "Клавиша niri"),
        app.settings
            .preferences
            .bindings
            .get(index)
            .map_or("", |binding| crate::remote::command_name(binding.action)),
        app.text(
            "Apply globals through niri's configuration.",
            "Глобальные применяются через конфиг niri."
        )
    );
    let description_height = (Paragraph::new(description.as_str())
        .wrap(Wrap { trim: false })
        .line_count(inner.width.max(1)) as u16)
        .min(footer_y.saturating_sub(description_y));
    frame.render_widget(
        Paragraph::new(description)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, description_y, inner.width, description_height),
    );
    let keys: Vec<Key> = Key::shortcut_choices().collect();
    let columns = (inner.width / 8).max(1) as usize;
    let key_y = description_y + description_height;
    let capacity = columns * usize::from(footer_y.saturating_sub(key_y));
    app.view.settings.binding_offset = app
        .view
        .settings
        .binding_offset
        .min(keys.len().saturating_sub(capacity.max(1)));
    for (visible, key) in keys
        .iter()
        .copied()
        .skip(app.view.settings.binding_offset)
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
                app.view.settings.binding_offset > 0,
            ),
            (
                "↓".into(),
                Target::DialogScroll(5),
                app.view.settings.binding_offset + capacity < keys.len(),
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
