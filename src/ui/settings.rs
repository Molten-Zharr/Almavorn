use super::{
    theme::Palette,
    widgets::{block, button, buttons, visible_offset},
};
use crate::app::{App, Hit, SettingsFocus, SettingsPage, Target};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier},
    widgets::{Clear, Paragraph, Wrap},
};

pub(super) fn render_settings(frame: &mut Frame, app: &mut App, palette: Palette) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let profile = app
        .settings
        .profiles
        .iter()
        .find(|p| p.id == app.settings.active_profile)
        .map(|p| p.name.as_str())
        .unwrap_or("");
    let outer = block(
        format!(
            "Almavorn / {} / {}",
            app.text("Settings", "Настройки"),
            profile
        ),
        palette,
        true,
    );
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let footer_height = inner.height.min(6);
    let body_height = inner.height.saturating_sub(footer_height);
    let menu_width = (inner.width / 4).clamp(1, 24);
    let menu = Rect::new(inner.x, inner.y, menu_width, body_height);
    let content = Rect::new(
        menu.right().saturating_add(1).min(inner.right()),
        inner.y,
        inner.width.saturating_sub(menu_width + 1),
        body_height,
    );
    app.settings_view.menu_area = menu;
    app.settings_view.parameters_area = content;
    frame.render_widget(
        Paragraph::new(app.text("Sections", "Разделы")).style(
            palette
                .text()
                .fg(palette.sidebar_title)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(menu.x, menu.y, menu.width, 1.min(menu.height)),
    );
    let page_index = SettingsPage::ALL
        .iter()
        .position(|page| *page == app.settings_view.page)
        .unwrap_or(0);
    let menu_offset = visible_offset(
        0,
        page_index,
        menu.height.saturating_sub(2) as usize,
        SettingsPage::ALL.len(),
    );
    for (index, page) in SettingsPage::ALL.into_iter().enumerate().skip(menu_offset) {
        let y = menu.y.saturating_add((index - menu_offset) as u16 + 2);
        if y >= menu.bottom() {
            break;
        }
        let selected = app.settings_view.page == page;
        let row = Rect::new(menu.x, y, menu.width, 1);
        let style = if selected {
            palette
                .text()
                .bg(palette.sidebar_selection)
                .fg(palette.background)
                .add_modifier(Modifier::BOLD)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                if selected && app.settings_view.focus == SettingsFocus::Menu {
                    ">"
                } else {
                    " "
                },
                page.name(app.settings.language)
            ))
            .style(style),
            row,
        );
        app.hits.push(Hit {
            area: row,
            target: Target::SettingsPage(page),
            enabled: true,
        });
    }
    if menu.width > 0 && body_height > 0 {
        frame.render_widget(
            Paragraph::new("│\n".repeat(usize::from(body_height)))
                .style(palette.text().fg(palette.separator)),
            Rect::new(
                menu.right(),
                menu.y,
                1.min(inner.right().saturating_sub(menu.right())),
                body_height,
            ),
        );
    }
    let rows = app.settings_rows();
    app.settings_view.selected = app.settings_view.selected.min(rows.len().saturating_sub(1));
    let capacity = (content.height.saturating_sub(2) / 4).max(1) as usize;
    app.settings_view.offset = visible_offset(
        app.settings_view.offset,
        app.settings_view.selected,
        capacity,
        rows.len(),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}  {}/{}",
            app.settings_view.page.name(app.settings.language),
            app.settings_view.selected + 1,
            rows.len()
        ))
        .style(
            palette
                .text()
                .fg(palette.help_heading)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(content.x, content.y, content.width, content.height.min(1)),
    );
    for (index, item) in rows
        .iter()
        .enumerate()
        .skip(app.settings_view.offset)
        .take(capacity)
    {
        let y = content
            .y
            .saturating_add(2 + ((index - app.settings_view.offset) * 4) as u16);
        if y >= content.bottom() {
            break;
        }
        let height = 4.min(content.bottom() - y);
        let row = Rect::new(content.x, y, content.width, height);
        let selected = app.settings_view.selected == index
            && app.settings_view.focus == SettingsFocus::Parameters;
        let style = if selected {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        frame.render_widget(Paragraph::new(" ").style(style), row);
        app.hits.push(Hit {
            area: row,
            target: Target::SettingSelect(index),
            enabled: true,
        });
        let value_width = ((item.value.chars().count() + if item.adjustable { 10 } else { 4 })
            as u16)
            .min(content.width / 2)
            .max(1)
            .min(content.width);
        let label_width = content.width.saturating_sub(value_width + 1);
        frame.render_widget(
            Paragraph::new(format!(
                "{}{}",
                if selected { "> " } else { "" },
                item.label
            ))
            .style(
                style
                    .fg(if item.enabled {
                        palette.text
                    } else {
                        palette.muted
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Rect::new(row.x, row.y, label_width, 1),
        );
        let mut value = Rect::new(
            content.right().saturating_sub(value_width),
            y,
            value_width,
            1,
        );
        if item.adjustable && value.width >= 7 {
            button(
                frame,
                app,
                Rect::new(value.x, value.y, 3, 1),
                "<",
                Target::SettingAdjust(index, -1),
                item.enabled,
                palette,
            );
            button(
                frame,
                app,
                Rect::new(value.right() - 3, value.y, 3, 1),
                ">",
                Target::SettingAdjust(index, 1),
                item.enabled,
                palette,
            );
            value.x += 3;
            value.width -= 6;
        }
        button(
            frame,
            app,
            value,
            &item.value,
            Target::Setting(index),
            item.enabled,
            palette,
        );
        if let Some([r, g, b]) = item.color {
            let swatch_x = row.x + label_width;
            frame.render_widget(
                Paragraph::new("■").style(style.fg(Color::Rgb(r, g, b))),
                Rect::new(
                    swatch_x.min(row.right()),
                    y,
                    u16::from(swatch_x < row.right()),
                    1,
                ),
            );
        }
        if height > 1 {
            frame.render_widget(
                Paragraph::new(item.description.clone())
                    .style(style.fg(palette.muted))
                    .wrap(Wrap { trim: false }),
                Rect::new(row.x, y + 1, row.width, (height - 1).min(2)),
            );
        }
    }
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height,
    );
    if footer.height > 0 {
        frame.render_widget(
            Paragraph::new(app.notice.clone()).style(palette.text().fg(if app.notice_error {
                palette.error
            } else {
                palette.muted
            })),
            Rect::new(footer.x, footer.y, footer.width, 1),
        );
    }
    if footer.height > 2 {
        let mut legend = app
            .text(
                "Tab: panel  ↑↓: select  ←→: change\nEnter: edit/apply  F1: help  Esc: back",
                "Tab: панель  ↑↓: выбор  ←→: значение\nEnter: изменить  F1: описание  Esc: назад",
            )
            .to_owned();
        if matches!(
            app.settings_view.page,
            SettingsPage::Profiles | SettingsPage::Themes | SettingsPage::Palettes
        ) {
            legend.push_str(app.text(
                "\nIns: create  F3: rename  Del: delete",
                "\nIns: создать  F3: имя  Del: удалить",
            ));
        }
        if app.settings_view.page == SettingsPage::Palettes {
            legend.push_str(app.text(
                "  Ctrl+I: import  Ctrl+E: export",
                "  Ctrl+I: импорт  Ctrl+E: экспорт",
            ));
        }
        frame.render_widget(
            Paragraph::new(legend)
                .style(palette.text().fg(palette.selection_text))
                .wrap(Wrap { trim: false }),
            Rect::new(
                footer.x,
                footer.y + 1,
                footer.width,
                footer.height.saturating_sub(3),
            ),
        );
    }
    if footer.height >= 2 {
        let selected = app.settings_view.selected;
        let enabled = rows.get(selected).is_some_and(|row| row.enabled);
        let adjustable = rows
            .get(selected)
            .is_some_and(|row| row.adjustable && row.enabled);
        let close_width =
            ((app.text("Close", "Закрыть").chars().count() + 3) as u16).min(footer.width);
        button(
            frame,
            app,
            Rect::new(
                footer.right() - close_width,
                footer.bottom() - 1,
                close_width,
                1,
            ),
            app.text("Close", "Закрыть"),
            Target::CloseDialog,
            true,
            palette,
        );
        buttons(
            frame,
            app,
            Rect::new(
                footer.x,
                footer.bottom() - 2,
                footer.width.saturating_sub(close_width),
                2,
            ),
            vec![
                ("<".into(), Target::SettingAdjust(selected, -1), adjustable),
                (">".into(), Target::SettingAdjust(selected, 1), adjustable),
                ("↑".into(), Target::DialogScroll(-1), true),
                ("↓".into(), Target::DialogScroll(1), true),
                (
                    app.text("Edit/apply", "Изменить").into(),
                    Target::Setting(selected),
                    enabled,
                ),
                ("?".into(), Target::SettingHelp(selected), true),
            ],
            palette,
        );
    }
}
