use crate::{
    app::{App, DownloadAction as A, DownloadsView, Hit, Target},
    downloads::{Engine, Status},
    ui::{
        theme::Palette,
        widgets::{buttons, buttons_height, clean, file_modal, visible_offset},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Paragraph, Wrap},
};
use std::sync::atomic::Ordering;

fn control(label: &str, action: A, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Downloads(action), enabled)
}
pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut DownloadsView,
    area: Rect,
    palette: Palette,
) {
    let inner = file_modal(
        frame,
        app,
        area,
        app.text("Downloads", "Загрузки").into(),
        48,
        palette,
    );
    if inner.width < 24 || inner.height < 18 {
        return;
    }
    let loading = app.downloads_loading();
    let selected = app.downloads.rows.get(view.selected);
    let active = selected.is_some_and(|r| app.downloads.active.as_deref() == Some(&r.id));
    let controls = if view.remove.is_some() {
        vec![
            control(app.text("Cancel [Enter]", "Отмена [Enter]"), A::Back, true),
            control(
                app.text("Remove [D]", "Удалить [D]"),
                A::ConfirmRemove,
                true,
            ),
        ]
    } else if view.search {
        vec![
            control(
                app.text("Download [Enter]", "Скачать [Enter]"),
                A::Queue,
                !loading && view.selected < app.downloads.results.len(),
            ),
            control(app.text("Search [F]", "Поиск [F]"), A::Search, !loading),
            control(app.text("Cancel", "Отменить"), A::Cancel, loading),
            control(app.text("History", "Загрузки"), A::History, true),
        ]
    } else {
        vec![
            control(
                app.text("Add link [A]", "Добавить ссылку [A]"),
                A::New,
                app.downloads.initialized,
            ),
            control(
                app.text("Find music [F]", "Найти музыку [F]"),
                A::Search,
                !loading,
            ),
            control(
                app.text("Edit [E]", "Изменить [E]"),
                A::Edit,
                selected.is_some_and(|r| r.status != Status::Done) && !active,
            ),
            control(
                app.text("Retry [R]", "Повторить [R]"),
                A::Retry,
                !app.downloads.initialized
                    || selected
                        .is_some_and(|r| matches!(r.status, Status::Failed | Status::Cancelled)),
            ),
            control(
                app.text("Cancel [C]", "Отменить [C]"),
                A::Cancel,
                active || loading || selected.is_some_and(|r| r.status == Status::Queued),
            ),
            control(
                app.text("To desk", "На стол"),
                A::Import,
                selected.is_some_and(|r| r.status == Status::Done),
            ),
            control(
                app.text("Remove from history", "Удалить из истории"),
                A::Remove,
                selected.is_some() && !active,
            ),
            control(
                app.text("Settings [S]", "Настройки [S]"),
                A::Settings,
                !loading,
            ),
            control(
                if app.downloads.config.engine == Engine::YtDlp {
                    "yt-dlp ↔ Cobalt"
                } else {
                    "Cobalt ↔ yt-dlp"
                },
                A::Engine,
                !loading,
            ),
            control(
                app.text(
                    "Install / update components",
                    "Установить / обновить компоненты",
                ),
                A::Tools,
                !loading && app.downloads.active.is_none(),
            ),
            control(app.text("Close", "Закрыть"), A::Back, true),
        ]
    };
    let height = buttons_height(app, inner.width, &controls)
        .max(2)
        .min(inner.height - 4);
    let footer = Rect::new(inner.x, inner.bottom() - height, inner.width, height);
    if view.remove.is_some() {
        frame.render_widget(
            Paragraph::new(app.text(
                "Remove this entry from download history? The audio file will stay in its folder.",
                "Удалить запись из истории загрузок? Аудиофайл останется в своей папке.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, footer.y - inner.y),
        );
        buttons(frame, app, footer, controls, palette);
        return;
    }
    let caption = if view.search {
        format!("{}: {}", app.text("Search", "Поиск"), app.downloads.query)
    } else {
        format!(
            "{} · {}",
            if app.downloads.config.engine == Engine::Cobalt {
                "Cobalt"
            } else {
                "yt-dlp"
            },
            app.downloads.config.directory.display()
        )
    };
    frame.render_widget(
        Paragraph::new(clean(&caption)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let busy = loading || app.downloads.active.is_some();
    let hint = if busy {
        let bytes = app.downloads.progress.load(Ordering::Relaxed);
        if bytes > 0 {
            format!(
                "{} · {:.1} MB",
                app.text("Working…", "Выполняется…"),
                bytes as f64 / 1048576.
            )
        } else {
            app.text(
                "Working… You can close this window.",
                "Выполняется… Можно закрыть это окно.",
            )
            .into()
        }
    } else {
        app.text(
            "Downloaded music goes to the sorting desk.",
            "Скачанная музыка попадает на сортировочный стол.",
        )
        .into()
    };
    frame.render_widget(
        Paragraph::new(hint).style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let error = app
        .downloads
        .error
        .as_ref()
        .or_else(|| selected.and_then(|r| r.error.as_ref()));
    let error_height = if error.is_some() {
        3.min(footer.y.saturating_sub(inner.y + 3))
    } else {
        0
    };
    let body = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        footer.y.saturating_sub(inner.y + 3 + error_height),
    );
    let count = if view.search {
        app.downloads.results.len()
    } else {
        app.downloads.rows.len()
    };
    view.selected = view.selected.min(count.saturating_sub(1));
    view.offset = visible_offset(view.offset, view.selected, body.height as usize, count);
    for (row, index) in (view.offset..count).take(body.height as usize).enumerate() {
        let text = if view.search {
            let hit = &app.downloads.results[index];
            format!("{} — {}", hit.artist, hit.title)
        } else {
            let job = &app.downloads.rows[index];
            let status = match job.status {
                Status::Queued => app.text("Queued", "В очереди"),
                Status::Running | Status::Publishing => app.text("Downloading", "Скачивается"),
                Status::Done => app.text("Saved", "Скачано"),
                Status::Failed => app.text("Failed", "Ошибка"),
                Status::Cancelled => app.text("Cancelled", "Отменено"),
            };
            format!(
                "{status} · {}",
                if job.name.is_empty() {
                    &job.url
                } else {
                    &job.name
                }
            )
        };
        let area = Rect::new(body.x, body.y + row as u16, body.width, 1);
        frame.render_widget(
            Paragraph::new(clean(&text)).style(if index == view.selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            }),
            area,
        );
        app.view.hits.push(Hit {
            area,
            target: Target::Downloads(A::Row(index)),
            enabled: true,
        });
    }
    if count == 0 && !loading {
        frame.render_widget(
            Paragraph::new(if view.search {
                app.text(
                    "No recordings found. Try another query.",
                    "Записи не найдены. Измените запрос.",
                )
            } else {
                app.text(
                    "Add a link or find music by title.",
                    "Добавьте ссылку или найдите музыку по названию.",
                )
            })
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            body,
        );
    }
    if let Some(error) = error {
        frame.render_widget(
            Paragraph::new(clean(error))
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    buttons(frame, app, footer, controls, palette);
}
