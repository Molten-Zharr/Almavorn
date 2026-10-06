use super::{
    theme::Palette,
    widgets::{block, button, button_width, buttons, visible_offset},
};
use crate::app::{App, Hit, SettingControl, SettingsFocus, SettingsPage, Target};
use crate::preferences::PlaybackTimeline;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier},
    text::Span,
    widgets::{Clear, Gauge, Paragraph, Wrap},
};

pub(super) fn render_settings(frame: &mut Frame, app: &mut App, palette: Palette) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    let outer = block(String::new(), palette, true);
    let mut inner = outer.inner(area);
    frame.render_widget(outer, area);
    breadcrumbs(frame, app, area, palette);
    if inner.y == area.y && inner.height > 0 {
        inner.y += 1;
        inner.height -= 1;
    }
    let footer_height = inner.height.min(6);
    let body_height = inner.height.saturating_sub(footer_height);
    let menu_width = (inner.width / 4).clamp(1, 24);
    let menu = Rect::new(inner.x, inner.y, menu_width, body_height);
    let mut content = Rect::new(
        menu.right().saturating_add(1).min(inner.right()),
        inner.y,
        inner.width.saturating_sub(menu_width + 1),
        body_height,
    );
    app.view.settings.menu_area = menu;
    app.view.settings.subtab_area = Rect::default();
    frame.render_widget(
        Paragraph::new(app.text("Sections", "Разделы")).style(
            palette
                .text()
                .fg(palette.sidebar_title)
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(menu.x, menu.y, menu.width, 1.min(menu.height)),
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
        let y = menu.y.saturating_add((index - menu_offset) as u16 + 2);
        if y >= menu.bottom() {
            break;
        }
        let selected = app.view.settings.page.section() == page;
        let row = Rect::new(menu.x, y, menu.width, 1);
        let style = if selected {
            palette
                .text()
                .bg(palette.sidebar_selection)
                .fg(palette.background)
                .add_modifier(Modifier::BOLD)
        } else if app.hovered(row) {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{} {}",
                if selected && app.view.settings.focus == SettingsFocus::Menu {
                    ">"
                } else {
                    " "
                },
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
    if app.view.settings.page.section() == SettingsPage::Themes {
        let height = theme_tabs(frame, app, content, palette);
        app.view.settings.subtab_area = Rect::new(content.x, content.y, content.width, height);
        let reserved = (height + 1).min(content.height);
        content.y += reserved;
        content.height -= reserved;
    }
    app.view.settings.parameters_area = content;
    let rows = app.settings_rows();
    app.view.settings.selected = app.view.settings.selected.min(rows.len().saturating_sub(1));
    let capacity = (content.height.saturating_sub(2) / 4).max(1) as usize;
    app.view.settings.offset = visible_offset(
        app.view.settings.offset,
        app.view.settings.selected,
        capacity,
        rows.len(),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}  {}/{}",
            app.view.settings.page.tab_name(app.settings.language),
            app.view.settings.selected + 1,
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
        .skip(app.view.settings.offset)
        .take(capacity)
    {
        let y = content
            .y
            .saturating_add(2 + ((index - app.view.settings.offset) * 4) as u16);
        if y >= content.bottom() {
            break;
        }
        let height = 4.min(content.bottom() - y);
        let row = Rect::new(content.x, y, content.width, height);
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
        if matches!(item.control, SettingControl::Volume) {
            volume(frame, app, row, index, &item.label, style, palette);
            continue;
        }
        if matches!(item.control, SettingControl::PlaybackTimeline) {
            frame.render_widget(
                Paragraph::new(format!(
                    "{}{}",
                    if selected { "> " } else { "" },
                    item.label
                ))
                .style(style.add_modifier(Modifier::BOLD)),
                Rect::new(row.x, row.y, row.width, 1),
            );
            let used = buttons(
                frame,
                app,
                Rect::new(row.x, row.y + 1, row.width, height.saturating_sub(1).min(2)),
                [PlaybackTimeline::Waveform, PlaybackTimeline::Progress]
                    .into_iter()
                    .map(|choice| {
                        let current = app.settings.appearance.playback_timeline == choice;
                        (
                            format!(
                                "{} {}",
                                if current { "●" } else { "○" },
                                choice.name(app.settings.language)
                            ),
                            Target::SettingsTimeline(index, choice),
                            true,
                        )
                    })
                    .collect(),
                palette,
            );
            let description_height = height.saturating_sub(used + 1);
            if description_height > 0 {
                frame.render_widget(
                    Paragraph::new(item.description.clone())
                        .style(style.fg(palette.muted))
                        .wrap(Wrap { trim: false }),
                    Rect::new(row.x, row.y + used + 1, row.width, description_height),
                );
            }
            continue;
        }
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
            if item.adjustable {
                Target::SettingAdjust(index, 1)
            } else {
                Target::Setting(index)
            },
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
            Paragraph::new(app.view.notice.clone()).style(palette.text().fg(
                if app.view.notice_error {
                    palette.error
                } else {
                    palette.muted
                },
            )),
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
            app.view.settings.page,
            SettingsPage::Profiles | SettingsPage::Themes | SettingsPage::Palettes
        ) {
            legend.push_str(app.text(
                "\nIns: create  F3: rename  Del: delete",
                "\nIns: создать  F3: имя  Del: удалить",
            ));
        }
        if app.view.settings.page == SettingsPage::Palettes {
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
        let selected = app.view.settings.selected;
        let enabled = rows.get(selected).is_some_and(|row| row.enabled);
        let adjustable = rows
            .get(selected)
            .is_some_and(|row| row.adjustable && row.enabled);
        let close_width =
            super::widgets::button_width(app, app.text("Close", "Закрыть"), &Target::CloseDialog)
                .min(footer.width);
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
        let mut style = palette
            .text()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD);
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
        let label = page.tab_name(app.settings.language);
        let width = button_width(app, label, &Target::SettingsTab(page)).min(area.width);
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if width == 0 || y >= area.bottom() {
            break;
        }
        let row = Rect::new(x, y, width, 1);
        button(
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
                Paragraph::new(format!("│{label}│")).style(
                    palette
                        .text()
                        .bg(palette.sidebar_selection)
                        .fg(palette.background)
                        .add_modifier(Modifier::BOLD),
                ),
                row,
            );
        }
        x = x.saturating_add(width).saturating_add(1);
    }
    if area.width == 0 || area.height == 0 {
        0
    } else {
        (y.saturating_sub(area.y) + 1).min(area.height)
    }
}

fn volume(
    frame: &mut Frame,
    app: &mut App,
    row: Rect,
    index: usize,
    label: &str,
    style: ratatui::style::Style,
    palette: Palette,
) {
    frame.render_widget(
        Paragraph::new(label).style(style.fg(palette.text).add_modifier(Modifier::BOLD)),
        Rect::new(row.x, row.y, row.width, row.height.min(1)),
    );
    if row.height < 3 {
        return;
    }
    let scale = Rect::new(
        row.x,
        row.y + 1,
        row.width,
        row.height.saturating_sub(2).min(2),
    );
    frame.render_widget(
        Gauge::default()
            .ratio(f64::from(app.settings.volume).clamp(0.0, 1.0))
            .label("")
            .gauge_style(style.fg(palette.accent).bg(palette.selection)),
        scale,
    );
    app.view.hits.push(Hit {
        area: scale,
        target: Target::SettingsVolume(index, scale),
        enabled: true,
    });
    let percentage = Rect::new(row.x, scale.bottom(), row.width, 1);
    frame.render_widget(
        Paragraph::new(format!("{}%", (app.settings.volume * 100.0).round() as u32))
            .alignment(Alignment::Center)
            .style(style.fg(palette.text)),
        percentage,
    );
    app.view.hits.push(Hit {
        area: percentage,
        target: Target::SettingAdjust(index, 1),
        enabled: true,
    });
}
