use crate::{
    app::{App, Form, FormAction as A, FormPurpose, Hit, Target},
    downloads::Engine,
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

fn control(label: impl Into<String>, action: A, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Form(action), enabled)
}

fn field(
    frame: &mut Frame,
    app: &mut App,
    form: &Form,
    index: usize,
    area: Rect,
    palette: Palette,
) {
    let field = &form.fields[index];
    let focused = form.selected == index;
    let border = block(field.label.clone(), palette, focused);
    let content = border.inner(area);
    frame.render_widget(border, area);
    let displayed = if field.secret {
        "•".repeat(field.value.chars().count())
    } else {
        clean(&field.value)
    };
    let characters: Vec<_> = displayed.chars().collect();
    let cursor = form.cursor.min(characters.len());
    let mut start = 0;
    let mut column = 0;
    if focused {
        start = cursor;
        while start > 0 {
            let width = ratatui::text::Span::raw(characters[start - 1].to_string()).width();
            if column + width >= usize::from(content.width) {
                break;
            }
            column += width;
            start -= 1;
        }
    }
    let text: String = characters[start..].iter().collect();
    frame.render_widget(
        Paragraph::new(text).style(if focused && form.selected_all {
            palette.text().bg(palette.selection)
        } else {
            palette.text()
        }),
        content,
    );
    if focused && content.width > 0 && content.height > 0 {
        frame.set_cursor_position(Position::new(content.x + column as u16, content.y));
    }
    app.view.hits.push(Hit {
        area,
        target: Target::Form(A::Field(index)),
        enabled: true,
    });
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    form: &mut Form,
    area: Rect,
    palette: Palette,
) {
    let new = matches!(form.purpose, FormPurpose::DownloadNew(_));
    let settings = matches!(form.purpose, FormPurpose::DownloadSettings);
    let width = area.width.saturating_sub(4).min(88).saturating_sub(2);
    let description = if new {
        app.text(
            "Paste a track or video link. The audio will appear on the sorting desk.",
            "Вставьте ссылку на трек или видео. Аудиофайл появится на сортировочном столе.",
        )
    } else {
        &form.description
    };
    let description_height = Paragraph::new(description)
        .wrap(Wrap { trim: false })
        .line_count(width.max(1))
        .min(4) as u16
        + 1;
    let folder_controls = vec![
        control(
            app.text("Choose folder · Ctrl+F", "Выбрать папку · Ctrl+F"),
            A::DownloadFolder,
            true,
        ),
        control(
            app.text("Downloads · Ctrl+D", "В Загрузки · Ctrl+D"),
            A::DownloadDefaultFolder,
            true,
        ),
    ];
    let cobalt = new && form.fields[3].value == "true";
    let engine_controls = vec![
        control(
            format!("{} yt-dlp · Ctrl+1", if cobalt { "○" } else { "●" }),
            A::DownloadEngine(Engine::YtDlp),
            true,
        ),
        control(
            format!("{} Cobalt · Ctrl+2", if cobalt { "●" } else { "○" }),
            A::DownloadEngine(Engine::Cobalt),
            true,
        ),
        control(
            app.text("Settings", "Настройки"),
            A::DownloadSettings,
            !app.downloads_loading(),
        ),
    ];
    let cobalt_help = app.text(
        "Get the API address and key from a Cobalt server owner. There is no public API. Guides for your own server:",
        "Адрес API и ключ дает владелец сервера Cobalt. Общедоступного API нет. Инструкции для своего сервера:",
    );
    let cobalt_controls = vec![
        control(
            app.text("API address guide · Ctrl+1", "Адрес API: инструкция · Ctrl+1"),
            A::CobaltApiGuide,
            true,
        ),
        control(
            app.text("API key guide · Ctrl+2", "Ключ API: инструкция · Ctrl+2"),
            A::CobaltKeyGuide,
            true,
        ),
    ];
    let settings_hint = app.text(
        "An empty key keeps the saved key. Missing folders are created automatically.",
        "Пустое поле ключа сохраняет прежний ключ. Папка создается автоматически.",
    );
    let cobalt_help_height = Paragraph::new(cobalt_help)
        .wrap(Wrap { trim: false })
        .line_count(width.max(1)) as u16;
    let cobalt_controls_height = buttons_height(app, width, &cobalt_controls);
    let settings_hint_height = Paragraph::new(settings_hint)
        .wrap(Wrap { trim: false })
        .line_count(width.max(1)) as u16;
    let footer_controls = vec![
        control(
            if new {
                app.text("Download · Enter", "Скачать · Enter")
            } else if settings {
                app.text("Save · Ctrl+S", "Сохранить · Ctrl+S")
            } else {
                app.text("Find · Enter", "Найти · Enter")
            },
            A::Save,
            !app.downloads_loading(),
        ),
        control(app.text("Cancel · Esc", "Отмена · Esc"), A::Back, true),
    ];
    let folder_height = buttons_height(app, width, &folder_controls);
    let engine_height = buttons_height(app, width, &engine_controls);
    let required_height = description_height
        + if new || settings {
            9 + folder_height
        } else {
            3
        }
        + if new {
            3 + engine_height
        } else if settings {
            cobalt_help_height + cobalt_controls_height + settings_hint_height
        } else {
            0
        }
        + 3
        + buttons_height(app, width, &footer_controls);
    let inner = file_modal(
        frame,
        app,
        area,
        form.title.clone(),
        required_height + 2,
        palette,
    );
    let footer_height = buttons_height(app, inner.width, &footer_controls).min(inner.height);
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height,
    );
    if inner.width < 30 || inner.height < required_height {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window. Esc: back.",
                "Увеличьте окно. Esc: назад.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        buttons(
            frame,
            app,
            footer,
            vec![control(app.text("Back", "Назад"), A::Back, true)],
            palette,
        );
        return;
    }
    frame.render_widget(
        Paragraph::new(description)
            .style(palette.text())
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, description_height),
    );
    let mut y = inner.y + description_height;
    for index in 0..if new || settings { 3 } else { 1 } {
        field(
            frame,
            app,
            form,
            index,
            Rect::new(inner.x, y, inner.width, 3),
            palette,
        );
        y += 3;
        if (new && index == 1) || (settings && index == 0) {
            y += buttons(
                frame,
                app,
                Rect::new(inner.x, y, inner.width, folder_height),
                folder_controls.clone(),
                palette,
            );
        }
        if settings && index == 0 {
            frame.render_widget(
                Paragraph::new(cobalt_help)
                    .style(palette.text().fg(palette.muted))
                    .wrap(Wrap { trim: false }),
                Rect::new(inner.x, y, inner.width, cobalt_help_height),
            );
            y += cobalt_help_height;
            y += buttons(
                frame,
                app,
                Rect::new(inner.x, y, inner.width, cobalt_controls_height),
                cobalt_controls.clone(),
                palette,
            );
        }
    }
    if new {
        let title = if form.selected == 3 {
            app.text("Service · Enter: change", "Сервис · Enter: сменить")
        } else {
            app.text("Download with", "Скачать через")
        };
        frame.render_widget(
            Paragraph::new(title).style(palette.text().fg(palette.muted)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        y += buttons(
            frame,
            app,
            Rect::new(inner.x, y, inner.width, engine_height),
            engine_controls,
            palette,
        );
        let hint = if cobalt && app.downloads.config.cobalt_url.trim().is_empty() {
            app.text(
                "Cobalt needs an API address. Open Settings or choose yt-dlp.",
                "Для Cobalt нужен адрес API. Откройте Настройки или выберите yt-dlp.",
            )
        } else if cobalt {
            app.text(
                "Cobalt receives the link and downloads the audio.",
                "Cobalt получит ссылку и скачает аудио.",
            )
        } else {
            app.text("yt-dlp runs on this computer. Required components are installed automatically.", "yt-dlp работает на этом компьютере. Нужные компоненты устанавливаются автоматически.")
        };
        frame.render_widget(
            Paragraph::new(hint)
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, 2),
        );
        y += 2;
    } else if settings {
        frame.render_widget(
            Paragraph::new(settings_hint)
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, settings_hint_height),
        );
        y += settings_hint_height;
    }
    let remaining = footer.y.saturating_sub(y);
    if let Some(error) = &form.error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, remaining),
        );
    } else if app.settings.preferences.appearance.show_button_shortcuts {
        let hint = if app.view.graphical_keycaps {
            app.text(
                "Ctrl+V: paste · Ctrl+A: select all · Tab: next field",
                "Ctrl+V: вставить · Ctrl+A: выделить все · Tab: следующее поле",
            )
        } else {
            app.text(
                "Ctrl+Shift+V: paste · Ctrl+A: select all · Tab: next field",
                "Ctrl+Shift+V: вставить · Ctrl+A: выделить все · Tab: следующее поле",
            )
        };
        frame.render_widget(
            Paragraph::new(hint)
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, remaining),
        );
    }
    buttons(frame, app, footer, footer_controls, palette);
}
