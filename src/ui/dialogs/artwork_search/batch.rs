use super::*;
use crate::ui::widgets::buttons_height;
use crate::{app::Hit, artwork::search::batch::Outcome};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let inner = modal(
        frame,
        app,
        area,
        app.text("Artwork search", "Поиск обложек").into(),
        30,
        palette,
    );
    if inner.is_empty() {
        return;
    }
    let running = app.artwork_batch.running;
    if inner.width < 48 || inner.height < 24 {
        let summary = if app.artwork_batch.started {
            app.artwork_batch.summary(app.settings.preferences.language)
        } else {
            String::new()
        };
        let text = format!("{}: {}\n{}\n\n{}\n{}\n\n{}",
            app.text("Tracks in this list", "Композиций в списке"), app.artwork_manual.tracks.len(),
            app.text("Existing covers are kept. Closing this window keeps the search running.", "Имеющиеся обложки останутся. Окно можно закрыть, поиск продолжится."),
            status(app), summary,
            app.text("R: start · C: stop · M: manual · I: details · U: current selection · Esc: close. Enlarge the window to view the queue.", "R: запуск · C: стоп · M: вручную · I: подробнее · U: текущие отметки · Esc: закрыть. Увеличьте окно для просмотра очереди."),
        );
        frame.render_widget(
            Paragraph::new(text)
                .style(palette.text())
                .wrap(Wrap { trim: true }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(3),
            ),
        );
        let height = inner.height.min(3);
        buttons(
            frame,
            app,
            Rect::new(inner.x, inner.bottom() - height, inner.width, height),
            vec![
                (
                    app.text("Find and save all", "Найти и сохранить все")
                        .into(),
                    Target::ArtworkSearch(A::StartAll),
                    !running && !app.artwork_manual.tracks.is_empty(),
                ),
                (
                    app.text("Stop", "Остановить").into(),
                    Target::ArtworkSearch(A::StopAll),
                    running && !app.artwork_batch.stopping,
                ),
                (
                    app.text("Close", "Закрыть").into(),
                    Target::CloseDialog,
                    true,
                ),
            ],
            palette,
        );
        return;
    }
    let language = app.settings.preferences.language;
    let count = app.artwork_manual.tracks.len();
    let selected = app.artwork_manual.track_index;
    let intro = app.text(
        "Find and save missing covers for this list. Existing covers are kept. Closing the window keeps the search running.",
        "Найти и сохранить недостающие обложки для списка. Имеющиеся останутся. Окно можно закрыть, поиск продолжится.",
    );
    let intro = Paragraph::new(intro)
        .style(palette.text())
        .wrap(Wrap { trim: true });
    let intro_height = intro.line_count(inner.width).min(4) as u16;
    frame.render_widget(
        intro,
        Rect::new(inner.x, inner.y, inner.width, intro_height),
    );
    let mut y = inner.y + intro_height + 1;
    frame.render_widget(
        Paragraph::new(format!(
            "{}: {count}",
            app.text("Tracks in this list", "Композиций в списке")
        ))
        .style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, y, inner.width, 1),
    );
    y += 1;
    let controls = vec![
        (
            app.text("Find and save all", "Найти и сохранить все")
                .into(),
            Target::ArtworkSearch(A::StartAll),
            !running && count > 0,
        ),
        (
            app.text("Stop", "Остановить").into(),
            Target::ArtworkSearch(A::StopAll),
            running && !app.artwork_batch.stopping,
        ),
        (
            app.text("Close", "Закрыть").into(),
            Target::CloseDialog,
            true,
        ),
    ];
    let height = buttons_height(app, inner.width, &controls);
    buttons(
        frame,
        app,
        Rect::new(inner.x, y, inner.width, height),
        controls,
        palette,
    );
    y += height;
    let controls = vec![
        (
            app.text("Current selection", "Текущие отметки").into(),
            Target::ArtworkSearch(A::RefreshTracks),
            !running,
        ),
        (
            app.text("Choose manually", "Выбрать вручную").into(),
            Target::ArtworkSearch(A::Manual),
            !running && count > 0,
        ),
        (
            app.text("Details", "Подробнее").into(),
            Target::ArtworkSearch(A::Details),
            true,
        ),
    ];
    let height = buttons_height(app, inner.width, &controls);
    buttons(
        frame,
        app,
        Rect::new(inner.x, y, inner.width, height),
        controls,
        palette,
    );
    y += height + 1;
    let paragraph = Paragraph::new(status(app))
        .style(palette.text().fg(palette.accent))
        .wrap(Wrap { trim: true });
    let height = paragraph.line_count(inner.width).min(2) as u16;
    frame.render_widget(paragraph, Rect::new(inner.x, y, inner.width, height));
    y += height;
    if running
        && let Some(progress) = app.artwork_batch.progress
        && let Some(retry) = progress.retry
    {
        let text = format!(
            "MusicBrainz: HTTP {} · {} {}/3 · {} {} {}",
            retry.status,
            app.text("attempt", "попытка"),
            retry.attempt,
            app.text("pause", "пауза"),
            retry.wait_seconds,
            app.text("s", "с")
        );
        let paragraph = Paragraph::new(text)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true });
        let height = paragraph.line_count(inner.width).min(2) as u16;
        frame.render_widget(paragraph, Rect::new(inner.x, y, inner.width, height));
        y += height;
    }
    if app.artwork_batch.started {
        let summary = Paragraph::new(app.artwork_batch.summary(language))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true });
        let height = summary.line_count(inner.width).min(3) as u16;
        frame.render_widget(summary, Rect::new(inner.x, y, inner.width, height));
        y += height;
    }
    let footer = inner.bottom().saturating_sub(4);
    let list = Rect::new(inner.x, y + 1, inner.width, footer.saturating_sub(y + 1));
    if list.height > 0 {
        let offset = visible_offset(0, selected, usize::from(list.height), count);
        for (index, track) in app
            .artwork_manual
            .tracks
            .iter()
            .enumerate()
            .skip(offset)
            .take(usize::from(list.height))
        {
            let report = app
                .artwork_batch
                .reports
                .get(index)
                .and_then(Option::as_ref);
            let status = report
                .map(|report| report.label(language).to_owned())
                .unwrap_or_else(|| {
                    if running && index == app.artwork_batch.current {
                        app.artwork_batch.stage.label(language)
                    } else {
                        app.text("Queued", "В очереди").into()
                    }
                });
            let rect = Rect::new(list.x, list.y + (index - offset) as u16, list.width, 1);
            let style = if index == selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else if matches!(report, Some(Outcome::Failed(_))) {
                palette.text().fg(palette.error)
            } else {
                palette.text()
            };
            let status_width = inner.width.min(24);
            let name_width = inner.width.saturating_sub(status_width + 1);
            let title = format!(
                "{}. {} - {}",
                index + 1,
                clean(&track.artist),
                clean(&track.title)
            );
            frame.render_widget(
                Paragraph::new(title).style(style),
                Rect::new(rect.x, rect.y, name_width, 1),
            );
            frame.render_widget(
                Paragraph::new(status).style(style),
                Rect::new(rect.right() - status_width, rect.y, status_width, 1),
            );
            app.view.hits.push(Hit {
                area: rect,
                target: Target::ArtworkSearch(A::Track(index)),
                enabled: true,
            });
        }
    }
    let detail = app
        .artwork_batch
        .reports
        .get(selected)
        .and_then(Option::as_ref)
        .map(|report| clean(&report.details(language)))
        .unwrap_or_else(|| {
            app.text(
                "Select a row for details. U updates the list from the playlist selection.",
                "Выберите строку для подробностей. U обновляет список по отметкам в плейлисте.",
            )
            .into()
        });
    frame.render_widget(
        Paragraph::new(detail)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: true }),
        Rect::new(inner.x, footer, inner.width, 2),
    );
    frame.render_widget(
        Paragraph::new(app.text(
            "R: start all · C: stop · M: manual · I: details · Esc: close",
            "R: запустить все · C: стоп · M: вручную · I: подробнее · Esc: закрыть",
        ))
        .style(palette.text().fg(palette.muted))
        .wrap(Wrap { trim: true }),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 2),
    );
}

fn status(app: &App) -> String {
    if app.artwork_batch.running {
        format!(
            "{}/{}: {}",
            app.artwork_batch.current + 1,
            app.artwork_manual.tracks.len(),
            if app.artwork_batch.stopping {
                app.text(
                    "Stopping after the current operation",
                    "Остановка после текущей операции",
                )
                .into()
            } else {
                app.artwork_batch
                    .stage
                    .label(app.settings.preferences.language)
            }
        )
    } else if app.artwork_batch.started {
        app.text("Search finished", "Поиск завершен").into()
    } else {
        app.text(
            "Ready to start. MusicBrainz + Cover Art Archive.",
            "Готово к запуску. MusicBrainz + Cover Art Archive.",
        )
        .into()
    }
}
