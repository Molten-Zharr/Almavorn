use crate::{
    app::{App, Hit, ModAction as A, ModsView, Target},
    ui::{
        theme::Palette,
        widgets::{buttons, buttons_height, clean, file_modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
fn action(label: &str, value: A, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Mods(value), enabled)
}
pub(super) fn permissions(app: &App, value: &crate::mods::Mod) -> String {
    let mut result = value
        .manifest
        .permissions
        .iter()
        .map(|p| match p.as_str() {
            "playback" => app.text("player controls", "управление плеером"),
            "library_read" => app.text("library titles", "названия из библиотеки"),
            "appearance" => app.text("palette changes", "смена палитры"),
            "network" => app.text("network requests", "запросы в сеть"),
            _ => p.as_str(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    if result.is_empty() {
        result = app
            .text("No additional access", "Без дополнительных разрешений")
            .into();
    }
    if !value.manifest.hosts.is_empty() {
        result.push_str(&format!(
            "\n{}: {}",
            app.text("Sources", "Источники"),
            value.manifest.hosts.join(", ")
        ));
    }
    result
}
pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut ModsView,
    area: Rect,
    palette: Palette,
) {
    let inner = file_modal(
        frame,
        app,
        area,
        app.text("Mods", "Моды").into(),
        48,
        palette,
    );
    render_content(frame, app, view, inner, palette);
}

pub(in crate::ui) fn render_content(
    frame: &mut Frame,
    app: &mut App,
    view: &mut ModsView,
    inner: Rect,
    palette: Palette,
) {
    if inner.width < 24 || inner.height < 18 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to manage mods.",
                "Увеличьте окно для управления модами.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let busy = app.mods_loading();
    let selected = app.mods.rows.get(view.selected);
    let mut controls = if view.remove.is_some() {
        vec![
            action(app.text("Cancel [Enter]", "Отмена [Enter]"), A::Back, true),
            action(
                app.text("Remove [D]", "Удалить [D]"),
                A::ConfirmRemove,
                !busy,
            ),
        ]
    } else if view.install.is_some() {
        vec![
            action(app.text("Cancel [Enter]", "Отмена [Enter]"), A::Back, true),
            action(
                app.text("Install [I]", "Установить [I]"),
                A::ConfirmInstall,
                !busy,
            ),
        ]
    } else {
        let mut items = vec![
            action(app.text("Create [N]", "Создать [N]"), A::New, !busy),
            action(app.text("Install [I]", "Установить [I]"), A::Install, !busy),
            action(
                if selected.is_some_and(|m| m.enabled) {
                    app.text("Disable [Space]", "Отключить [Space]")
                } else {
                    app.text("Enable [Space]", "Включить [Space]")
                },
                A::Enable,
                !busy && selected.is_some(),
            ),
            action(
                app.text("Description and actions [E]", "Описание и действия [E]"),
                A::Edit,
                !busy && selected.is_some(),
            ),
            action(
                app.text("Code [C]", "Код [C]"),
                A::Code,
                !busy && selected.is_some(),
            ),
            action(
                app.text("Settings [S]", "Настройки [S]"),
                A::Settings,
                !busy && selected.is_some(),
            ),
            action(
                app.text("Archive", "Архив"),
                A::Export,
                !busy && selected.is_some(),
            ),
            action(
                app.text("Remove", "Удалить"),
                A::Remove,
                !busy && selected.is_some(),
            ),
            action(app.text("Refresh", "Обновить список"), A::Refresh, !busy),
            action(
                app.text("Creation guide", "Как создать мод"),
                A::Guide,
                true,
            ),
        ];
        if view.opened {
            items.retain(|(_, target, _)| {
                !matches!(
                    target,
                    Target::Mods(A::New | A::Install | A::Refresh | A::Guide)
                )
            });
        }
        if !view.opened {
            items.push(action(
                app.text("Open [Enter]", "Открыть [Enter]"),
                A::Open,
                selected.is_some(),
            ));
        }
        items
    };
    if view.opened
        && view.install.is_none()
        && view.remove.is_none()
        && let Some(value) = selected
    {
        let mut custom = value
            .manifest
            .actions
            .iter()
            .enumerate()
            .skip(view.action)
            .take(4)
            .map(|(i, item)| {
                action(
                    &if i < 9 {
                        format!("{} [{}]", item.name, i + 1)
                    } else {
                        item.name.clone()
                    },
                    A::Run(i),
                    value.enabled,
                )
            })
            .collect::<Vec<_>>();
        if value.manifest.actions.len() > 4 {
            custom.push(action("←", A::ActionPage(-1), view.action > 0));
            custom.push(action(
                "→",
                A::ActionPage(1),
                view.action + 4 < value.manifest.actions.len(),
            ));
        }
        custom.append(&mut controls);
        controls = custom;
    }
    if busy && view.install.is_none() && view.remove.is_none() {
        controls.push(action(
            app.text("Cancel operation", "Отменить операцию"),
            A::Cancel,
            true,
        ));
    }
    let footer_height = buttons_height(app, inner.width, &controls)
        .max(2)
        .min(inner.height - 4);
    let footer = Rect::new(
        inner.x,
        inner.bottom() - footer_height,
        inner.width,
        footer_height,
    );
    let error_height = if app.mods.error.is_some() {
        3.min(footer.y.saturating_sub(inner.y + 1))
    } else {
        0
    };
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        footer.y.saturating_sub(inner.y + error_height),
    );
    if view.remove.is_some() {
        frame.render_widget(
            Paragraph::new(app.text(
                "Remove this mod and its saved settings?",
                "Удалить мод и его сохраненные настройки?",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            body,
        );
    } else if let Some(value) = &view.install {
        let replacing = app
            .mods
            .rows
            .iter()
            .any(|m| m.manifest.id == value.manifest.id);
        let text = format!(
            "{} · {}\n{}\n\n{}\n\n{}:\n{}",
            value.manifest.name,
            value.manifest.version,
            value.manifest.author,
            value.manifest.description,
            if replacing {
                app.text(
                    "Update mod. Requested access",
                    "Обновление мода. Запрашиваемый доступ",
                )
            } else {
                app.text("Requested access", "Запрашиваемый доступ")
            },
            permissions(app, value)
        );
        let paragraph = Paragraph::new(clean(&text))
            .style(palette.text())
            .wrap(Wrap { trim: false });
        app.view.dialog_page_size = usize::from(body.height);
        app.view.dialog_scroll_max = paragraph
            .line_count(body.width.max(1))
            .saturating_sub(body.height as usize);
        view.panel_offset = view.panel_offset.min(app.view.dialog_scroll_max);
        frame.render_widget(
            paragraph.scroll((view.panel_offset.min(u16::MAX as usize) as u16, 0)),
            body,
        );
    } else if view.opened
        && let Some(value) = selected
    {
        let text = format!(
            "{} · {}\n{}\n\n{}\n\n{}\n\n{}",
            value.manifest.name,
            value.manifest.version,
            value.manifest.author,
            value.manifest.description,
            permissions(app, value),
            app.mods
                .panels
                .get(&value.manifest.id)
                .map(String::as_str)
                .unwrap_or(app.text(
                    "Use a mod button below.",
                    "Нажмите кнопку действия мода ниже."
                ))
        );
        let paragraph = Paragraph::new(clean(&text))
            .style(palette.text())
            .wrap(Wrap { trim: false });
        app.view.dialog_page_size = usize::from(body.height);
        app.view.dialog_scroll_max = paragraph
            .line_count(body.width.max(1))
            .saturating_sub(body.height as usize);
        view.panel_offset = view.panel_offset.min(app.view.dialog_scroll_max);
        frame.render_widget(
            paragraph.scroll((view.panel_offset.min(u16::MAX as usize) as u16, 0)),
            body,
        );
    } else {
        let count = app.mods.rows.len();
        view.selected = view.selected.min(count.saturating_sub(1));
        view.offset = visible_offset(view.offset, view.selected, body.height as usize, count);
        for (row, index) in (view.offset..count).take(body.height as usize).enumerate() {
            let value = &app.mods.rows[index];
            let row = Rect::new(body.x, body.y + row as u16, body.width, 1);
            frame.render_widget(
                Paragraph::new(clean(&format!(
                    "{} {} · {}",
                    if value.enabled { "[x]" } else { "[ ]" },
                    value.manifest.name,
                    value.manifest.version
                )))
                .style(if index == view.selected {
                    palette
                        .text()
                        .bg(palette.selection)
                        .fg(palette.selection_text)
                } else {
                    palette.text()
                }),
                row,
            );
            app.view.hits.push(Hit {
                area: row,
                target: Target::Mods(A::Row(index)),
                enabled: true,
            });
        }
        if count == 0 {
            frame.render_widget(
                Paragraph::new(if busy {
                    app.text("Loading mods…", "Загрузка модов…")
                } else {
                    app.text(
                        "Install a mod archive or create your own.",
                        "Установите архив мода или создайте свой.",
                    )
                })
                .style(palette.text())
                .wrap(Wrap { trim: false }),
                body,
            );
        }
    }
    if let Some(error) = &app.mods.error {
        frame.render_widget(
            Paragraph::new(clean(error))
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    buttons(frame, app, footer, controls, palette);
}
