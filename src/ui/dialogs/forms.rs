use crate::{
    app::{App, Form, FormAction, Hit, Target},
    ui::{
        theme::Palette,
        widgets::{block, buttons, file_modal},
    },
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    form: &mut Form,
    area: Rect,
    palette: Palette,
) {
    if matches!(form.purpose, crate::app::FormPurpose::DownloadNew(_) | crate::app::FormPurpose::DownloadSettings | crate::app::FormPurpose::DownloadSearch) && !form.discard {
        super::download_form::render(frame, app, form, area, palette);
        return;
    }
    if matches!(form.purpose, crate::app::FormPurpose::SpotifyConnect(..)) && !form.discard {
        super::spotify::render(frame, app, form, area, palette);
        return;
    }
    let inner = file_modal(frame, app, area, form.title.clone(), 44, palette);
    if inner.width < 20 || inner.height < 12 {
        return;
    }
    let footer = Rect::new(inner.x, inner.bottom() - 3, inner.width, 3);
    if form.discard {
        frame.render_widget(
            Paragraph::new(app.text(
                "Discard unsaved changes?",
                "Отменить несохраненные изменения?",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        buttons(
            frame,
            app,
            footer,
            vec![
                (
                    app.text("Return [Enter]", "Вернуться [Enter]").into(),
                    Target::Form(FormAction::Back),
                    true,
                ),
                (
                    app.text("Discard changes [D]", "Отменить изменения [D]")
                        .into(),
                    Target::Form(FormAction::Discard),
                    true,
                ),
            ],
            palette,
        );
        return;
    }
    let description_height = Paragraph::new(form.description.clone())
        .wrap(Wrap { trim: false })
        .line_count(inner.width)
        .min(4) as u16;
    frame.render_widget(
        Paragraph::new(form.description.clone())
            .style(palette.text())
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, description_height),
    );
    let hint_height = 2;
    let error_height = if form.error.is_some() { 3 } else { 0 };
    let description_gap = u16::from(
        description_height > 0
            && footer.y.saturating_sub(inner.y + description_height + hint_height + error_height)
                > 3 * form.fields.len().max(1) as u16,
    );
    let available = footer
        .y
        .saturating_sub(inner.y + description_height + description_gap + hint_height + error_height);
    let height = (available / form.fields.len().max(1) as u16).max(3);
    let visible = (available / height).max(1) as usize;
    let start = form.selected.saturating_sub(visible - 1);
    let mut y = inner.y + description_height + description_gap;
    for index in start..(start + visible).min(form.fields.len()) {
        let field = &form.fields[index];
        let area = Rect::new(
            inner.x,
            y,
            inner.width,
            height.min(footer.y.saturating_sub(y + hint_height + error_height)),
        );
        if area.height < 3 {
            break;
        }
        let border = block(field.label.clone(), palette, index == form.selected);
        let content = border.inner(area);
        frame.render_widget(border, area);
        let displayed = if field.checkbox {
            format!(
                "{} {}",
                if field.value == "true" { "[x]" } else { "[ ]" },
                app.text("Enter: toggle", "Enter: переключить")
            )
        } else if field.secret {
            "•".repeat(field.value.chars().count())
        } else {
            field.value.clone()
        };
        let before = displayed.chars().take(form.cursor).collect::<String>();
        let cursor_line = Paragraph::new(before.clone())
            .wrap(Wrap { trim: false })
            .line_count(content.width.max(1))
            .saturating_sub(1);
        let scroll = if index == form.selected {
            cursor_line.saturating_sub(content.height.saturating_sub(1) as usize)
        } else {
            0
        };
        frame.render_widget(
            Paragraph::new(displayed)
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0))
                .style(if index == form.selected && form.selected_all {
                    palette.text().bg(palette.selection)
                } else {
                    palette.text()
                }),
            content,
        );
        if index == form.selected && content.width > 0 && content.height > 0 {
            let column = before
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                % usize::from(content.width);
            frame.set_cursor_position(Position::new(
                content.x + column as u16,
                content.y
                    + cursor_line
                        .saturating_sub(scroll)
                        .min(content.height as usize - 1) as u16,
            ));
        }
        app.view.hits.push(Hit {
            area,
            target: Target::Form(if field.checkbox {
                FormAction::ToggleField(index)
            } else {
                FormAction::Field(index)
            }),
            enabled: true,
        });
        y += area.height;
    }
    if let Some(error) = &form.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - 5, inner.width, 3),
        );
    }
    if app.settings.preferences.appearance.show_button_shortcuts { frame.render_widget(
        Paragraph::new(app.text(
            "Tab: fields · Ctrl+A: select all · Ctrl+S: save",
            "Tab: поля · Ctrl+A: выделить все · Ctrl+S: сохранить",
        ))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: false }),
        Rect::new(inner.x, footer.y - 2, inner.width, 2),
    ); }
    buttons(
        frame,
        app,
        footer,
        vec![
            (
                app.text("Apply", "Применить").into(),
                Target::Form(FormAction::Save),
                true,
            ),
            (
                app.text("Back", "Назад").into(),
                Target::Form(FormAction::Back),
                true,
            ),
        ],
        palette,
    );
}
