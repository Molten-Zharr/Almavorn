use super::{
    buttons::{button_width, quiet_button, quiet_width},
    theme::Palette,
    widgets::{block, visible_offset},
};
use crate::app::{App, Hit, SettingControl, SettingsFocus, SettingsPage, Target};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph, Wrap},
};

pub(super) fn render_settings(frame: &mut Frame, app: &mut App, palette: Palette) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let outer =
        block(String::new(), palette, false).border_style(palette.text().fg(palette.separator));
    let mut inner = outer.inner(area);
    frame.render_widget(outer, area);
    breadcrumbs(frame, app, area, palette);
    if inner.y == area.y && inner.height > 0 {
        inner.y += 1;
        inner.height -= 1;
    }
    app.view.settings.menu_area = Rect::default();
    app.view.settings.subtab_area = Rect::default();
    app.view.settings.parameters_area = Rect::default();
    if area.width < 32 || area.height < 14 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to edit settings.",
                "Увеличьте окно для редактирования настроек.",
            ))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(1),
            ),
        );
        quiet_button(
            frame,
            app,
            Rect::new(
                inner.x,
                inner.bottom().saturating_sub(1),
                inner.width,
                inner.height.min(1),
            ),
            app.text("Back", "Назад"),
            Target::CloseDialog,
            true,
            palette,
        );
        return;
    }
    let footer_height = inner.height.min(3);
    let body_height = inner.height.saturating_sub(footer_height);
    let min_menu_width = if inner.width < 45 { 10 } else { 12 };
    let menu_width = (inner.width / 5)
        .clamp(min_menu_width, 20)
        .min(inner.width / 2);
    let menu = Rect::new(inner.x, inner.y, menu_width, body_height);
    let mut content = Rect::new(
        menu.right() + 2,
        inner.y,
        inner.width.saturating_sub(menu_width + 3),
        body_height,
    );
    app.view.settings.menu_area = menu;
    frame.render_widget(
        Paragraph::new(app.text("Sections", "Разделы")).style(palette.text().fg(palette.muted)),
        Rect::new(menu.x, menu.y, menu.width, 1),
    );
    let page_index = SettingsPage::SECTIONS
        .iter()
        .position(|page| *page == app.view.settings.page.section())
        .unwrap_or(0);
    let menu_offset = visible_offset(
        0,
        page_index,
        menu.height.saturating_sub(2) as usize,
        SettingsPage::SECTIONS.len(),
    );
    for (index, page) in SettingsPage::SECTIONS
        .into_iter()
        .enumerate()
        .skip(menu_offset)
    {
        let y = menu.y + (index - menu_offset) as u16 + 2;
        if y >= menu.bottom() {
            break;
        }
        let row = Rect::new(menu.x, y, menu.width, 1);
        let selected = app.view.settings.page.section() == page;
        let style = if selected {
            palette
                .text()
                .bg(palette.sidebar_selection)
                .fg(palette.background)
        } else if app.hovered(row) {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{}{}",
                if menu.width < 12 { " " } else { "  " },
                page.name(app.settings.language)
            ))
            .style(style),
            row,
        );
        app.view.hits.push(Hit {
            area: row,
            target: Target::SettingsPage(page),
            enabled: true,
        });
    }
    frame.render_widget(
        Paragraph::new("│\n".repeat(usize::from(body_height)))
            .style(palette.text().fg(palette.separator)),
        Rect::new(menu.right(), menu.y, 1, body_height),
    );
    if app.view.settings.page.section() == SettingsPage::Themes {
        let height = theme_tabs(frame, app, content, palette);
        app.view.settings.subtab_area = Rect::new(content.x, content.y, content.width, height);
        let reserved = (height + 1).min(content.height);
        content.y += reserved;
        content.height -= reserved;
    } else {
        frame.render_widget(
            Paragraph::new(app.view.settings.page.name(app.settings.language))
                .style(palette.text().fg(palette.sidebar_title)),
            Rect::new(content.x, content.y, content.width, 1),
        );
        content.y += 2;
        content.height = content.height.saturating_sub(2);
    }
    app.view.settings.parameters_area = content;
    let rows = app.settings_rows();
    app.view.settings.selected = app.view.settings.selected.min(rows.len().saturating_sub(1));
    let stacked = content.width < 40;
    let grouped = app.view.settings.page == SettingsPage::Shortcuts;
    let step = if grouped && !stacked { 1 } else { 2 };
    let row_height = if stacked { 2 } else { 1 };
    let capacity = if stacked {
        content.height / step
    } else {
        content.height.div_ceil(step)
    } as usize;
    let value_width = rows
        .iter()
        .map(|item| {
            if matches!(item.control, SettingControl::Volume) {
                36
            } else {
                Span::raw(&item.value).width() as u16 + 8
            }
        })
        .max()
        .unwrap_or(24)
        .max(24)
        .min(if stacked {
            content.width
        } else {
            content.width / 2
        });
    if grouped {
        let selected = app.view.settings.selected;
        let mut offset = app.view.settings.offset.min(selected);
        while offset < selected {
            let mut previous_group = None;
            let height = rows[offset..=selected]
                .iter()
                .map(|row| {
                    let divider = u16::from(
                        row.shortcut_group.is_some() && row.shortcut_group != previous_group,
                    ) * 2;
                    previous_group = row.shortcut_group;
                    usize::from(step + divider)
                })
                .sum::<usize>()
                - usize::from(step - row_height);
            if height <= usize::from(content.height) {
                break;
            }
            offset += 1;
        }
        app.view.settings.offset = offset;
    } else {
        app.view.settings.offset = visible_offset(
            app.view.settings.offset,
            app.view.settings.selected,
            capacity,
            rows.len(),
        );
    }
    let mut y = content.y;
    let mut previous_group = None;
    for (index, item) in rows.iter().enumerate().skip(app.view.settings.offset) {
        if let Some(group) = item.shortcut_group
            && item.shortcut_group != previous_group
        {
            // Dividers are decoration; setting indices and navigation skip them.
            if y + 2 + row_height > content.bottom() {
                break;
            }
            let title = format!("─ {} ", group.name(app.settings.language));
            let remaining = usize::from(content.width).saturating_sub(Span::raw(&title).width());
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(title, palette.text().fg(palette.sidebar_title)),
                    Span::styled("─".repeat(remaining), palette.text().fg(palette.separator)),
                ]))
                .wrap(Wrap { trim: true }),
                Rect::new(content.x, y, content.width, 2),
            );
            y += 2;
        }
        previous_group = item.shortcut_group;
        if y + row_height > content.bottom() {
            break;
        }
        let row = Rect::new(content.x, y, content.width, row_height);
        y += step;
        let selected = app.view.settings.selected == index
            && app.view.settings.focus == SettingsFocus::Parameters;
        let style = if selected || (item.enabled && app.hovered(row)) {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        frame.render_widget(Paragraph::new(" ").style(style), row);
        app.view.hits.push(Hit {
            area: row,
            target: Target::SettingSelect(index),
            enabled: true,
        });
        let label_width = if stacked {
            content
                .width
                .saturating_sub(u16::from(item.color.is_some()))
        } else {
            content.width.saturating_sub(value_width + 2)
        };
        frame.render_widget(
            Paragraph::new(item.label.as_str()).style(style.fg(if item.enabled {
                palette.text
            } else {
                palette.muted
            })),
            Rect::new(row.x, row.y, label_width, 1),
        );
        let value = Rect::new(
            content.right() - value_width,
            row.y + u16::from(stacked),
            value_width,
            1,
        );
        if matches!(item.control, SettingControl::Volume) {
            volume(frame, app, value, index, style, palette);
            continue;
        }
        let mut middle = value;
        if middle.width >= Span::raw(&item.value).width() as u16 + 8 {
            if item.adjustable {
                quiet_button(
                    frame,
                    app,
                    Rect::new(value.x, value.y, 3, 1),
                    "<",
                    Target::SettingAdjust(index, -1),
                    item.enabled,
                    palette,
                );
                quiet_button(
                    frame,
                    app,
                    Rect::new(value.right() - 3, value.y, 3, 1),
                    ">",
                    Target::SettingAdjust(index, 1),
                    item.enabled,
                    palette,
                );
            }
            middle.x += 3;
            middle.width -= 6;
        }
        setting_value(
            frame,
            app,
            Hit {
                area: middle,
                target: Target::Setting(index),
                enabled: item.enabled,
            },
            &item.value,
            style,
            palette,
        );
        if let Some([r, g, b]) = item.color {
            let swatch = Rect::new(row.x + label_width, row.y, 1, 1);
            frame.render_widget(
                Paragraph::new("■").style(style.fg(Color::Rgb(r, g, b))),
                swatch,
            );
            app.view.hits.push(Hit {
                area: swatch,
                target: Target::Setting(index),
                enabled: item.enabled,
            });
        }
    }
    let footer = Rect::new(
        inner.x,
        inner.bottom() - footer_height,
        inner.width,
        footer_height,
    );
    let description = if app.view.notice_error {
        app.view.notice.as_str()
    } else {
        rows.get(app.view.settings.selected)
            .map_or("", |item| item.description.as_str())
    };
    frame.render_widget(
        Paragraph::new(description)
            .style(palette.text().fg(if app.view.notice_error {
                palette.error
            } else {
                palette.muted
            }))
            .wrap(Wrap { trim: false }),
        Rect::new(
            content.x,
            footer.y,
            inner.right().saturating_sub(content.x),
            footer.height.saturating_sub(1),
        ),
    );
    let selected = app.view.settings.selected;
    let mut actions = vec![
        ("↑".to_owned(), Target::DialogScroll(-1), true),
        ("↓".to_owned(), Target::DialogScroll(1), true),
    ];
    if footer.width >= 65 {
        actions.push((
            app.text("Edit", "Изменить").into(),
            Target::Setting(selected),
            rows.get(selected).is_some_and(|item| item.enabled),
        ));
    }
    actions.push((
        if footer.width < 40 {
            "?"
        } else {
            app.text("Info", "Описание")
        }
        .into(),
        Target::SettingHelp(selected),
        true,
    ));
    actions.push((
        if footer.width < 40 {
            "←"
        } else {
            app.text("Back", "Назад")
        }
        .into(),
        Target::CloseDialog,
        true,
    ));
    let action_width = actions
        .iter()
        .map(|(label, target, _)| button_width(app, label, target) + 1)
        .sum::<u16>()
        .saturating_sub(1);
    let mut x = footer.right().saturating_sub(action_width).max(footer.x);
    let mut status = app.text("Tab: panels", "Tab: панели").to_owned();
    if footer.width >= 100 {
        status.push_str(" · ");
        status.push_str(if app.settings_saving() {
            app.text("Saving…", "Сохранение…")
        } else {
            app.text("Autosave", "Автосохранение")
        });
    }
    if x.saturating_sub(footer.x) < Span::raw(&status).width() as u16 {
        status = "Tab".into();
    }
    frame.render_widget(
        Paragraph::new(status).style(palette.text().fg(palette.muted)),
        Rect::new(footer.x, footer.bottom() - 1, x.saturating_sub(footer.x), 1),
    );
    for (label, target, enabled) in actions {
        let width = button_width(app, &label, &target).min(footer.right().saturating_sub(x));
        quiet_button(
            frame,
            app,
            Rect::new(x, footer.bottom() - 1, width, 1),
            &label,
            target,
            enabled,
            palette,
        );
        x = x.saturating_add(width + 1);
    }
}

fn breadcrumbs(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if area.height == 0 {
        return;
    }
    let inset = if palette.borders == crate::preferences::BorderWeight::None {
        0
    } else {
        2
    };
    let width = area.width.saturating_sub(inset * 2);
    let mut x = area.x + inset.min(area.width);
    let right = x + width;
    let profile = app
        .settings
        .profiles
        .iter()
        .find(|profile| profile.id == app.settings.active_profile)
        .map(|profile| profile.name.clone())
        .unwrap_or_default();
    let page = app.view.settings.page;
    let mut entries = vec![
        ("Almavorn".to_owned(), Target::CloseDialog),
        (
            app.text("Settings", "Настройки").to_owned(),
            Target::SettingsPage(SettingsPage::General),
        ),
        (profile, Target::SettingsPage(SettingsPage::Profiles)),
        (
            page.section().name(app.settings.language).to_owned(),
            Target::SettingsPage(page.section()),
        ),
    ];
    if page.section() == SettingsPage::Themes {
        entries.push((
            page.tab_name(app.settings.language).to_owned(),
            Target::SettingsTab(page),
        ));
    }
    for (index, (label, target)) in entries.into_iter().enumerate() {
        if index > 0 {
            let separator = Rect::new(x, area.y, 3.min(right.saturating_sub(x)), 1);
            frame.render_widget(
                Paragraph::new(" / ").style(palette.text().fg(palette.separator)),
                separator,
            );
            x = separator.right();
        }
        if x >= right {
            break;
        }
        let row = Rect::new(
            x,
            area.y,
            (Span::raw(&label).width() as u16).min(right - x),
            1,
        );
        let mut style = palette.text().fg(if index == 0 {
            palette.accent
        } else {
            palette.muted
        });
        if app.hovered(row) {
            style = style.bg(palette.selection);
        }
        frame.render_widget(Paragraph::new(label).style(style), row);
        app.view.hits.push(Hit {
            area: row,
            target,
            enabled: true,
        });
        x = row.right();
    }
}

fn theme_tabs(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) -> u16 {
    let mut x = area.x;
    let mut y = area.y;
    for page in SettingsPage::THEME_TABS {
        let label = if page == SettingsPage::Typography
            && area.width < quiet_width(page.tab_name(app.settings.language))
        {
            app.text("Font", "Шрифт")
        } else {
            page.tab_name(app.settings.language)
        };
        let width = quiet_width(label).min(area.width);
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if width == 0 || y >= area.bottom() {
            break;
        }
        let row = Rect::new(x, y, width, 1);
        quiet_button(
            frame,
            app,
            row,
            label,
            Target::SettingsTab(page),
            true,
            palette,
        );
        if page == app.view.settings.page {
            frame.render_widget(
                Paragraph::new(format!(" {label} ")).style(
                    palette
                        .text()
                        .bg(palette.sidebar_selection)
                        .fg(palette.background),
                ),
                row,
            );
        }
        x = x.saturating_add(width + 1);
    }
    if area.width == 0 || area.height == 0 {
        0
    } else {
        (y - area.y + 1).min(area.height)
    }
}

fn setting_value(
    frame: &mut Frame,
    app: &mut App,
    hit: Hit,
    label: &str,
    style: Style,
    palette: Palette,
) {
    let style = if hit.enabled && app.hovered(hit.area) {
        style.bg(palette.selection).fg(palette.accent)
    } else {
        style.fg(if hit.enabled {
            palette.button_text
        } else {
            palette.muted
        })
    };
    let text = if hit.area.width > Span::raw(label).width() as u16 {
        format!("{label} ")
    } else {
        label.to_owned()
    };
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Right)
            .style(style),
        hit.area,
    );
    app.view.hits.push(hit);
}

fn volume(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    index: usize,
    style: Style,
    palette: Palette,
) {
    if area.width < 12 {
        setting_value(
            frame,
            app,
            Hit {
                area,
                target: Target::Setting(index),
                enabled: true,
            },
            &format!("{}%", (app.settings.volume * 100.0).round() as u16),
            style,
            palette,
        );
        return;
    }
    quiet_button(
        frame,
        app,
        Rect::new(area.x, area.y, 3, 1),
        "<",
        Target::SettingAdjust(index, -1),
        true,
        palette,
    );
    quiet_button(
        frame,
        app,
        Rect::new(area.right() - 3, area.y, 3, 1),
        ">",
        Target::SettingAdjust(index, 1),
        true,
        palette,
    );
    let scale = Rect::new(area.x + 3, area.y, area.width.saturating_sub(12).min(24), 1);
    if scale.width > 0 {
        let point = (app.settings.volume.clamp(0.0, 1.0) * f32::from(scale.width.saturating_sub(1)))
            .round() as u16;
        let spans: Vec<_> = (0..scale.width)
            .map(|column| {
                Span::styled(
                    if column == point { "●" } else { "─" },
                    style.fg(if column <= point {
                        palette.accent
                    } else {
                        palette.separator
                    }),
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(Line::from(spans)), scale);
        app.view.hits.push(Hit {
            area: scale,
            target: Target::SettingsVolume(index, scale),
            enabled: true,
        });
    }
    setting_value(
        frame,
        app,
        Hit {
            area: Rect::new(area.right() - 8, area.y, 5, 1),
            target: Target::Setting(index),
            enabled: true,
        },
        &format!("{}%", (app.settings.volume * 100.0).round() as u16),
        style,
        palette,
    );
}
