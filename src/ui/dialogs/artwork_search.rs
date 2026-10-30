use super::super::{
    theme::Palette,
    widgets::{buttons, clean, modal, visible_offset},
};
use crate::app::{App, ArtworkSearchAction as A, Target};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if app.artwork_manual.details {
        return details(frame, app, area, palette);
    }
    if app.artwork_manual.confirm_save {
        return confirm(frame, app, area, palette);
    }
    let inner = modal(
        frame,
        app,
        area,
        app.text("Artwork search", "Поиск обложек").into(),
        30,
        palette,
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if inner.height < 18 || inner.width < 38 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to search for artwork.",
                "Увеличьте окно для поиска обложек.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: true }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            ),
        );
        buttons(
            frame,
            app,
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
            vec![(
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            )],
            palette,
        );
        return;
    }
    let count = app.artwork_manual.tracks.len();
    let index = app.artwork_manual.track_index;
    frame.render_widget(
        Paragraph::new(app.text(
            "Mark tracks in the playlist, then select one here and press Find.",
            "Отметьте композиции в плейлисте, выберите нужную и нажмите Найти.",
        ))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: true }),
        Rect::new(inner.x, inner.y, inner.width, 2),
    );
    frame.render_widget(
        Paragraph::new(app.text(
            "MusicBrainz receives the artist and title from tags or the filename.",
            "MusicBrainz получает исполнителя и название из тегов или имени файла.",
        ))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: true }),
        Rect::new(inner.x, inner.y + 2, inner.width, 2),
    );
    let track = app
        .artwork_manual
        .track
        .as_ref()
        .map(|track| {
            let artist = clean(&track.artist);
            let title = clean(&track.title);
            if artist.trim().is_empty() {
                title
            } else {
                format!("{artist} - {title}")
            }
        })
        .unwrap_or_else(|| {
            app.text(
                "Select a track in the playlist",
                "Выберите композицию в плейлисте",
            )
            .into()
        });
    frame.render_widget(
        Paragraph::new(format!(
            "{} {}/{}: {}",
            app.text("Track", "Композиция"),
            if count == 0 { 0 } else { index + 1 },
            count,
            track
        ))
        .style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y + 5, inner.width, 1),
    );
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y + 6, inner.width, 2),
        vec![
            (
                app.text("Previous track", "Предыдущая").into(),
                Target::ArtworkSearch(A::Track(index.saturating_sub(1))),
                index > 0 && !app.artwork_manual_saving(),
            ),
            (
                app.text("Next track", "Следующая").into(),
                Target::ArtworkSearch(A::Track(index + 1)),
                index + 1 < count && !app.artwork_manual_saving(),
            ),
            (
                app.text("Details", "Подробнее").into(),
                Target::ArtworkSearch(A::Details),
                true,
            ),
        ],
        palette,
    );
    let saving = app.artwork_manual_saving();
    let controls_height = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y + 8, inner.width, 2),
        vec![
            (
                if app.artwork_manual.error.is_some() {
                    app.text("Retry search", "Повторить поиск")
                } else {
                    app.text("Find", "Найти")
                }
                .into(),
                Target::ArtworkSearch(A::Search),
                app.artwork_manual.track.is_some() && !app.artwork_manual_searching() && !saving,
            ),
            (
                app.text("Save cover", "Сохранить обложку").into(),
                Target::ArtworkSearch(A::Save),
                app.artwork_manual
                    .preview
                    .as_ref()
                    .is_some_and(|preview| preview.index == app.artwork_manual.selected)
                    && !saving,
            ),
            (
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                !saving,
            ),
        ],
        palette,
    );
    let retry = app
        .artwork_manual
        .progress
        .and_then(|progress| progress.retry)
        .filter(|_| app.artwork_manual_searching());
    let footer_height = if app.artwork_manual.error.is_some()
        || app.artwork_manual.message.is_some()
        || app.artwork_manual.warning.is_some()
        || retry.is_some()
    {
        4
    } else {
        1
    };
    let footer_y = inner.bottom().saturating_sub(footer_height);
    let message = if saving {
        app.text("Saving artwork…", "Сохранение обложки…")
            .to_owned()
    } else if let Some(error) = &app.artwork_manual.error {
        format!("{}: {}", app.text("Error", "Ошибка"), clean(error))
    } else if let Some(message) = &app.artwork_manual.message {
        message.clone()
    } else if let Some(warning) = &app.artwork_manual.warning {
        clean(warning)
    } else if let Some(retry) = retry {
        format!(
            "MusicBrainz: HTTP {}. {} {}/3, {} {} {}.",
            retry.status,
            app.text("Attempt", "Попытка"),
            retry.attempt,
            app.text("pause", "пауза"),
            retry.wait_seconds,
            app.text("s", "с")
        )
    } else if app.artwork_manual.truncated {
        app.text(
            "Showing the first results. Select an edition to load its cover.",
            "Показаны первые результаты. Выберите издание, чтобы увидеть обложку.",
        )
        .into()
    } else {
        app.text(
            "R: find · ←→: track · ↑↓: edition · Enter: save · Esc: close",
            "R: найти · ←→: композиция · ↑↓: издание · Enter: сохранить · Esc: закрыть",
        )
        .into()
    };
    frame.render_widget(
        Paragraph::new(message)
            .style(palette.text().fg(if app.artwork_manual.error.is_some() {
                palette.error
            } else {
                palette.muted
            }))
            .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            footer_y,
            inner.width,
            if footer_height > 1 {
                footer_height - 1
            } else {
                1
            },
        ),
    );
    if app.artwork_manual.error.is_some() || app.artwork_manual.warning.is_some() {
        frame.render_widget(
            Paragraph::new(app.text("Full message: Details", "Полное сообщение: Подробнее"))
                .style(palette.text().fg(palette.muted)),
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        );
    }
    let heading_y = inner.y + 8 + controls_height;
    if !app.artwork_manual.results.is_empty() && heading_y < footer_y {
        frame.render_widget(
            Paragraph::new(format!(
                "{}: {}",
                app.text("Editions", "Издания"),
                app.artwork_manual.results.len()
            ))
            .style(palette.text().fg(palette.accent)),
            Rect::new(inner.x, heading_y, inner.width, 1),
        );
    }
    let start = heading_y + 1;
    let available = footer_y.saturating_sub(start);
    if available == 0 {
        return;
    }
    let wide = inner.width >= 60;
    let preview_width = if wide {
        (inner.width / 3).clamp(16, 30)
    } else {
        inner.width
    };
    let preview_height = if wide {
        available
    } else {
        (available / 2).min(10)
    };
    let list = Rect::new(
        inner.x,
        start,
        if wide {
            inner.width.saturating_sub(preview_width + 1)
        } else {
            inner.width
        },
        if wide {
            available
        } else {
            available.saturating_sub(preview_height + u16::from(preview_height > 0))
        },
    );
    let preview = if wide {
        Rect::new(list.right() + 1, start, preview_width, preview_height)
    } else {
        Rect::new(
            inner.x,
            list.bottom() + u16::from(preview_height > 0),
            preview_width,
            preview_height,
        )
    };
    if app.artwork_manual_searching() {
        let message = if let Some(progress) = app
            .artwork_manual
            .progress
            .filter(|progress| progress.total > 0)
        {
            format!(
                "{}: {}/{} · {}: {}",
                app.text("MusicBrainz checked", "MusicBrainz проверено"),
                progress.checked,
                progress.total,
                app.text("editions", "изданий"),
                progress.editions
            )
        } else {
            app.text("Searching MusicBrainz…", "Поиск в MusicBrainz…")
                .to_owned()
        };
        frame.render_widget(
            Paragraph::new(message).style(palette.text().fg(palette.muted)),
            list,
        );
    } else if app.artwork_manual.results.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.artwork_manual.track.is_some() {
                app.text(
                    "Choose Find to search for artwork.",
                    "Нажмите Найти, чтобы подобрать обложку.",
                )
            } else {
                app.text(
                    "Select a track before opening this window.",
                    "Выберите композицию перед открытием окна.",
                )
            })
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true }),
            list,
        );
    } else {
        let offset = visible_offset(
            0,
            app.artwork_manual.selected,
            usize::from(list.height),
            app.artwork_manual.results.len(),
        );
        for (index, candidate) in app
            .artwork_manual
            .results
            .iter()
            .enumerate()
            .skip(offset)
            .take(usize::from(list.height))
        {
            let label = format!(
                "{}{} · {}",
                clean(&candidate.album),
                if candidate.date.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", clean(&candidate.date))
                },
                clean(&candidate.artist)
            );
            let rect = Rect::new(list.x, list.y + (index - offset) as u16, list.width, 1);
            let style = if index == app.artwork_manual.selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            };
            frame.render_widget(Paragraph::new(label).style(style), rect);
            app.view.hits.push(crate::app::Hit {
                area: rect,
                target: Target::ArtworkSearch(A::Select(index)),
                enabled: !saving,
            });
        }
    }
    if app.artwork_manual_loading() {
        frame.render_widget(
            Paragraph::new(app.text(
                "Loading from Cover Art Archive…",
                "Загрузка из Cover Art Archive…",
            ))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true }),
            preview,
        );
    } else if let Some(image) = app
        .artwork_manual
        .preview
        .as_ref()
        .filter(|image| image.index == app.artwork_manual.selected)
    {
        draw_image(frame, preview, &image.image, palette);
    } else if !app.artwork_manual.results.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text("No preview", "Нет обложки"))
                .style(palette.text().fg(palette.muted)),
            preview,
        );
    }
}

fn details(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Search details", "Подробности поиска").into(),
        25,
        palette,
    );
    let mut text = app.text(
        "Select tracks in the playlist before opening this window. Choose a track and press Find. No other tracks are searched.\n\nMusicBrainz receives the artist and title from the tags or filename. Cover Art Archive provides the image. Select an edition to see its cover. Saving to an audio file requires confirmation.",
        "Перед открытием окна отметьте композиции в плейлисте. Выберите нужную и нажмите Найти. Другие композиции не ищутся.\n\nMusicBrainz получает исполнителя и название из тегов или имени файла. Cover Art Archive предоставляет изображение. Выберите издание, чтобы увидеть обложку. Для записи в аудиофайл потребуется подтверждение.",
    ).to_owned();
    if let Some(error) = &app.artwork_manual.error {
        text.push_str(&format!(
            "\n\n{}: {}",
            app.text("Error", "Ошибка"),
            clean(error)
        ));
    }
    if let Some(message) = &app.artwork_manual.message {
        text.push_str(&format!("\n\n{}", clean(message)));
    }
    if let Some(warning) = &app.artwork_manual.warning {
        text.push_str(&format!("\n\n{}", clean(warning)));
    }
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(2),
    );
    let paragraph = Paragraph::new(text)
        .style(palette.text())
        .wrap(Wrap { trim: false });
    app.view.dialog_scroll_max = paragraph
        .line_count(body.width.max(1))
        .saturating_sub(usize::from(body.height));
    app.artwork_manual.details_offset = app
        .artwork_manual
        .details_offset
        .min(app.view.dialog_scroll_max);
    frame.render_widget(
        paragraph.scroll((
            app.artwork_manual.details_offset.min(u16::MAX as usize) as u16,
            0,
        )),
        body,
    );
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(2),
            inner.width,
            inner.height.min(2),
        ),
        vec![
            (
                "↑".into(),
                Target::ArtworkSearch(A::ScrollDetails(-5)),
                app.artwork_manual.details_offset > 0,
            ),
            (
                "↓".into(),
                Target::ArtworkSearch(A::ScrollDetails(5)),
                app.artwork_manual.details_offset < app.view.dialog_scroll_max,
            ),
            (
                app.text("Back", "Назад").into(),
                Target::ArtworkSearch(A::CloseDetails),
                true,
            ),
        ],
        palette,
    );
}

fn draw_image(
    frame: &mut Frame,
    area: Rect,
    image: &crate::artwork::ArtworkImage,
    palette: Palette,
) {
    if area.is_empty() || image.width == 0 || image.height == 0 {
        return;
    }
    let rows = area
        .height
        .min((u32::from(area.width) * image.height / image.width.max(1) / 2).max(1) as u16);
    let cols = area
        .width
        .min((u32::from(rows) * 2 * image.width / image.height.max(1)).max(1) as u16);
    let background = if let Color::Rgb(r, g, b) = palette.background {
        [r, g, b]
    } else {
        [0, 0, 0]
    };
    let pixel = |x: u32, y: u32| {
        let index = ((y.min(image.height - 1) * image.width + x.min(image.width - 1)) * 4) as usize;
        let alpha = u16::from(image.rgba[index + 3]);
        let blend = |channel: usize| {
            ((u16::from(image.rgba[index + channel]) * alpha
                + u16::from(background[channel]) * (255 - alpha))
                / 255) as u8
        };
        Color::Rgb(blend(0), blend(1), blend(2))
    };
    let lines: Vec<Line> = (0..rows)
        .map(|row| {
            let top = u32::from(row) * 2 * image.height / (u32::from(rows) * 2);
            let bottom = (u32::from(row) * 2 + 1) * image.height / (u32::from(rows) * 2);
            Line::from(
                (0..cols)
                    .map(|column| {
                        let x = u32::from(column) * image.width / u32::from(cols);
                        Span::styled("▀", Style::default().fg(pixel(x, top)).bg(pixel(x, bottom)))
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), Rect::new(area.x, area.y, cols, rows));
}

fn confirm(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Save artwork", "Сохранить обложку").into(),
        10,
        palette,
    );
    let album = app
        .artwork_manual
        .results
        .get(app.artwork_manual.selected)
        .map(|item| clean(&item.album))
        .unwrap_or_default();
    let track = app
        .artwork_manual
        .track
        .as_ref()
        .map(|track| format!("{} - {}", clean(&track.artist), clean(&track.title)))
        .unwrap_or_default();
    let visible = inner.width >= 32 && inner.height >= 6;
    app.artwork_manual.confirmation_visible = visible;
    let question = app.text(
        "Save and replace existing front artwork?",
        "Сохранить и заменить лицевую обложку?",
    );
    let body = if visible {
        format!(
            "{question}\n{}: {track}\n{}: {album}",
            app.text("Track", "Композиция"),
            app.text("Edition", "Издание")
        )
    } else {
        app.text(
            "Enlarge the window to confirm saving.",
            "Увеличьте окно для подтверждения сохранения.",
        )
        .to_owned()
    };
    frame.render_widget(
        Paragraph::new(body)
            .style(palette.text())
            .wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(2),
        ),
    );
    buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.bottom().saturating_sub(2),
            inner.width,
            inner.height.min(2),
        ),
        vec![
            (
                app.text("Save", "Сохранить").into(),
                Target::ArtworkSearch(A::ConfirmSave),
                visible,
            ),
            (
                app.text("Cancel", "Отмена").into(),
                Target::ArtworkSearch(A::CancelSave),
                true,
            ),
        ],
        palette,
    );
}
