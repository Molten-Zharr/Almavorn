use crate::{
    app::{App, Dialog, SettingsPage, Target},
    ui::{
        dialogs::{help, mods, services},
        theme::Palette,
        widgets::{buttons, buttons_height, clean},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Modifier,
    text::Line,
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if app.settings_page() == SettingsPage::ServiceGuide {
        service_guide(frame, app, area, palette);
        return;
    }
    if let Some(text) = app.settings_guide() {
        let mut offset = app.view.settings.offset;
        help::copyable_text(frame, app, &mut offset, area, text, palette);
        app.view.settings.offset = offset;
        return;
    }
    // Dialogs retain selection and operation state; their content now lives in
    // Settings. Forms and confirmations draw over the same settings page.
    let base = app.view.dialog.as_ref().map(Dialog::settings_base);
    if app.settings_page() == SettingsPage::Mods {
        let mut view = match base {
            Some(Dialog::Mods(view)) => view.clone(),
            _ => Default::default(),
        };
        view.remove = None;
        view.install = None;
        mods::render_content(frame, app, &mut view, area, palette);
        if let Some(Dialog::Mods(current)) = &mut app.view.dialog
            && current.remove.is_none()
            && current.install.is_none()
        {
            *current = view;
        }
    } else {
        let mut view = match base {
            Some(Dialog::Services(view)) => view.clone(),
            _ => Default::default(),
        };
        view.remove = None;
        services::render_content(frame, app, &mut view, area, palette);
        if let Some(Dialog::Services(current)) = &mut app.view.dialog
            && current.remove.is_none()
        {
            *current = view;
        }
    }
}

fn service_guide(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let topics = app.service_guide_topics();
    let selected = app
        .view
        .settings
        .guide_topic
        .min(topics.len().saturating_sub(1));
    let controls: Vec<_> = topics
        .iter()
        .enumerate()
        .map(|(index, (title, _))| {
            (
                format!("{}{title}", if selected == index { "● " } else { "" }),
                Target::SettingsGuideTopic(index),
                true,
            )
        })
        .collect();
    let tabs_height = buttons_height(app, area.width, &controls).min(area.height);
    buttons(
        frame,
        app,
        Rect::new(area.x, area.y, area.width, tabs_height),
        controls,
        palette,
    );
    let footer_controls = vec![
        (
            app.text("Connections", "Подключения").into(),
            Target::SettingsPage(SettingsPage::Services),
            true,
        ),
        (
            app.text("Lyrics settings", "Настройки текстов песен")
                .into(),
            Target::SettingsPage(SettingsPage::Lyrics),
            true,
        ),
        (
            app.text("Copy section", "Копировать раздел").into(),
            Target::CopyHelp(None),
            true,
        ),
    ];
    let remaining = area.height.saturating_sub(tabs_height);
    let footer_height = buttons_height(app, area.width, &footer_controls).min(remaining);
    let gap = u16::from(remaining > footer_height);
    let hint_height = u16::from(remaining > footer_height + gap);
    let body = Rect::new(
        area.x,
        area.y + tabs_height + gap,
        area.width.min(100),
        remaining.saturating_sub(footer_height + gap + hint_height),
    );
    let (title, text) = topics.get(selected).copied().unwrap_or_default();
    let mut lines = vec![
        Line::styled(
            title,
            palette
                .text()
                .fg(palette.sidebar_title)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
    ];
    for line in text.lines() {
        let heading = line.strip_prefix("### ");
        lines.push(Line::styled(
            guide_text(heading.unwrap_or(line)),
            if heading.is_some() {
                palette
                    .text()
                    .fg(palette.sidebar_title)
                    .add_modifier(Modifier::BOLD)
            } else {
                palette.text()
            },
        ));
    }
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    app.view.dialog_page_size = body.height as usize;
    app.view.dialog_scroll_max = paragraph
        .line_count(body.width.max(1))
        .saturating_sub(body.height as usize)
        .min(u16::MAX as usize);
    app.view.settings.offset = app.view.settings.offset.min(app.view.dialog_scroll_max);
    frame.render_widget(paragraph.scroll((app.view.settings.offset as u16, 0)), body);
    frame.render_widget(
        Paragraph::new(app.text(
            "← →: topic · ↑ ↓: scroll · Ctrl+C: copy section",
            "← →: тема · ↑ ↓: прокрутка · Ctrl+C: копировать раздел",
        ))
        .style(palette.text().fg(palette.muted)),
        Rect::new(area.x, body.bottom(), area.width, hint_height),
    );
    buttons(
        frame,
        app,
        Rect::new(
            area.x,
            area.bottom().saturating_sub(footer_height),
            area.width,
            footer_height,
        ),
        footer_controls,
        palette,
    );
}

fn guide_text(line: &str) -> String {
    let mut result = String::new();
    let mut rest = line;
    while let Some(start) = rest.find('[') {
        let Some((label, tail)) = rest[start + 1..].split_once("](") else {
            break;
        };
        let Some((url, tail)) = tail.split_once(')') else {
            break;
        };
        result.push_str(&rest[..start]);
        result.push_str(label);
        result.push_str(" (");
        result.push_str(url);
        result.push(')');
        rest = tail;
    }
    result.push_str(rest);
    clean(&result.replace('`', ""))
}
