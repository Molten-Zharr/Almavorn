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
    let inner = file_modal(
        frame,
        app,
        area,
        form.title.clone(),
        if new || settings { 28 } else { 13 },
        palette,
    );
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
    let footer_height = buttons_height(app, inner.width, &footer_controls).min(inner.height);
    let footer = Rect::new(
        inner.x,
        inner.bottom().saturating_sub(footer_height),
        inner.width,
        footer_height,
    );
    if inner.width < 30 || inner.height < if new || settings { 23 } else { 9 } {
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
    let description = if new {
        app.text(
            "Paste a track or video link. The audio will appear on the sorting desk.",
            "Вставьте ссылку на трек или видео. Аудиофайл появится на сортировочном столе.",
        )
    } else {
        &form.description
    };
    frame.render_widget(
        Paragraph::new(description)
            .style(palette.text())
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, 2),
    );
    let mut y = inner.y + 2;
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
                Rect::new(inner.x, y, inner.width, 2),
                vec![
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
                ],
                palette,
            );
        }
    }
    if new {
        let cobalt = form.fields[3].value == "true";
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
            Rect::new(inner.x, y, inner.width, 3),
            vec![
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
            ],
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
            Paragraph::new(app.text(
                "An empty key keeps the saved key. Missing folders are created automatically.",
                "Пустое поле ключа сохраняет прежний ключ. Новая папка создается автоматически.",
            ))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, 2),
        );
        y += 2;
    }
    let remaining = footer.y.saturating_sub(y);
    if let Some(error) = &form.error {
        frame.render_widget(
            Paragraph::new(error.as_str())
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, y, inner.width, remaining),
        );
    } else {
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
