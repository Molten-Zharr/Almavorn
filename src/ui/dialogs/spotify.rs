use crate::{
    app::{App, Form, FormAction, FormPurpose, Hit, Target},
    ui::{
        theme::Palette,
        widgets::{block, buttons, buttons_height, clean, file_modal},
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
    let FormPurpose::SpotifyConnect(_, step) = form.purpose else {
        return;
    };
    let title = format!("{} · {}/3", form.title, step + 1);
    let inner = file_modal(frame, app, area, title, 24, palette);
    let mut controls = Vec::new();
    if step == 0 {
        controls.push((
            app.text("Open website", "Открыть сайт").into(),
            Target::Form(FormAction::SpotifyDashboard),
            true,
        ));
    } else if step == 1 {
        controls.push((
            app.text("Copy address", "Копировать адрес").into(),
            Target::Form(FormAction::SpotifyRedirect),
            true,
        ));
    }
    controls.push((
        app.text(
            if step == 2 { "Connect" } else { "Next" },
            if step == 2 {
                "Подключить"
            } else {
                "Дальше"
            },
        )
        .into(),
        Target::Form(if step == 2 {
            FormAction::Save
        } else {
            FormAction::SpotifyNext
        }),
        true,
    ));
    controls.push((
        app.text(
            if step == 0 { "Cancel" } else { "Back" },
            if step == 0 {
                "Отмена"
            } else {
                "Назад"
            },
        )
        .into(),
        Target::Form(FormAction::Back),
        true,
    ));
    let footer_height = buttons_height(app, inner.width, &controls).min(inner.height);
    if inner.width < 30 || inner.height < 10 + footer_height {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to connect Spotify. Esc: back.",
                "Увеличьте окно для подключения Spotify. Esc: назад.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        buttons(
            frame,
            app,
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            vec![(
                app.text("Back", "Назад").into(),
                Target::Form(FormAction::Back),
                true,
            )],
            palette,
        );
        return;
    }
    let footer = Rect::new(
        inner.x,
        inner.bottom() - footer_height,
        inner.width,
        footer_height,
    );
    let error_height = form.error.as_ref().map_or(0, |error| {
        Paragraph::new(error.as_str())
            .wrap(Wrap { trim: false })
            .line_count(inner.width)
            .min(3) as u16
    });
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        footer.y.saturating_sub(inner.y + error_height),
    );
    let description = match step {
        0 => app.text(
            "Spotify Premium is required for playback.\n\nClick Open website, then Create app in Spotify Developers. Name: Almavorn. Description: Music player. Save the app, return here and click Next.",
            "Для воспроизведения нужен Spotify Premium.\n\nНажмите Открыть сайт, затем Create app в Spotify Developers. Имя: Almavorn. Описание: Музыкальный плеер. Сохраните приложение, вернитесь сюда и нажмите Дальше."),
        1 => app.text(
            "Click Copy address.\n\nIn the app you created on the website, open Settings. Paste this address into Redirect URIs and click Save. Return here and click Next.",
            "Нажмите Копировать адрес.\n\nНа сайте откройте Settings созданного приложения. Вставьте этот адрес в Redirect URIs и нажмите Save. Вернитесь сюда и нажмите Дальше."),
        _ => app.text(
            "Copy Client ID from the app's page on Spotify Developers and paste it below. Click Connect to open sign-in in your browser.",
            "Скопируйте Client ID со страницы приложения в Spotify Developers и вставьте его сюда. Нажмите Подключить: браузер откроет вход в аккаунт."),
    };
    let reserved = if step == 2 {
        5
    } else if step == 1 {
        2
    } else {
        0
    };
    let description_area = Rect::new(
        body.x,
        body.y,
        body.width,
        body.height.saturating_sub(reserved),
    );
    if Paragraph::new(description)
        .wrap(Wrap { trim: false })
        .line_count(body.width)
        > usize::from(description_area.height)
    {
        let mut message = app
            .text(
                "Enlarge the window to show this step in full.",
                "Увеличьте окно, чтобы увидеть этот шаг целиком.",
            )
            .to_owned();
        if let Some(error) = &form.error {
            message.push_str("\n\n");
            message.push_str(error);
        }
        frame.render_widget(
            Paragraph::new(message)
                .style(palette.text())
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, footer.y - inner.y),
        );
        buttons(
            frame,
            app,
            footer,
            vec![(
                app.text("Back", "Назад").into(),
                Target::Form(FormAction::Back),
                true,
            )],
            palette,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(description)
            .style(palette.text())
            .wrap(Wrap { trim: false }),
        description_area,
    );
    if step == 1 {
        frame.render_widget(
            Paragraph::new(crate::services::SPOTIFY_REDIRECT)
                .style(palette.text().fg(palette.accent))
                .wrap(Wrap { trim: false }),
            Rect::new(body.x, body.bottom() - 2, body.width, 2),
        );
    } else if step == 2 {
        let field = Rect::new(body.x, body.bottom() - 5, body.width, 3);
        let border = block("Client ID".into(), palette, true);
        let input = border.inner(field);
        frame.render_widget(border, field);
        let scroll = form
            .cursor
            .saturating_sub(input.width.saturating_sub(1) as usize);
        let visible: String = form.fields[0].value.chars().skip(scroll).collect();
        frame.render_widget(
            Paragraph::new(clean(&visible)).style(if form.selected_all {
                palette.text().bg(palette.selection)
            } else {
                palette.text()
            }),
            input,
        );
        frame.set_cursor_position(Position::new(
            input.x
                + form
                    .cursor
                    .saturating_sub(scroll)
                    .min(input.width.saturating_sub(1) as usize) as u16,
            input.y,
        ));
        app.view.hits.push(Hit {
            area: field,
            target: Target::Form(FormAction::Field(0)),
            enabled: true,
        });
        let hint = if app.view.graphical_keycaps {
            app.text(
                "Ctrl+V: paste · Enter: connect",
                "Ctrl+V: вставить · Enter: подключить",
            )
        } else {
            app.text(
                "Ctrl+Shift+V: paste · Enter: connect",
                "Ctrl+Shift+V: вставить · Enter: подключить",
            )
        };
        frame.render_widget(
            Paragraph::new(hint)
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            Rect::new(body.x, body.bottom() - 2, body.width, 2),
        );
    }
    if let Some(error) = &form.error {
        frame.render_widget(
            Paragraph::new(error.clone())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    buttons(frame, app, footer, controls, palette);
}
