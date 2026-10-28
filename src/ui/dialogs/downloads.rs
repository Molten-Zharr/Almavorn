use crate::{
    app::{App, DownloadAction as A, DownloadsView, Hit, Target},
    downloads::{Engine, Status},
    ui::{
        theme::Palette,
        widgets::{button, button_width, buttons, buttons_height, clean, file_modal, visible_offset},
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

struct ActionGroup {
    title: &'static str,
    controls: Vec<(String, Target, bool)>,
}

impl ActionGroup {
    fn height(&self, app: &App, width: u16) -> u16 {
        if self.controls.is_empty() { 0 } else { 1 + buttons_height(app, width, &self.controls) }
    }

    fn render(self, frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
        if self.controls.is_empty() || area.height < 2 { return; }
        frame.render_widget(
            Paragraph::new(self.title).style(palette.text().fg(palette.help_heading)),
            Rect::new(area.x, area.y, area.width, 1),
        );
        buttons(frame, app, Rect::new(area.x, area.y + 1, area.width, area.height - 1), self.controls, palette);
    }
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
    if view.remove.is_some() {
        let controls = vec![
            control(app.text("Cancel [Enter]", "Отмена [Enter]"), A::Back, true),
            control(app.text("Remove [D]", "Удалить [D]"), A::ConfirmRemove, true),
        ];
        let height = buttons_height(app, inner.width, &controls);
        let footer = Rect::new(inner.x, inner.bottom() - height, inner.width, height);
        frame.render_widget(
            Paragraph::new(app.text(
                "Remove this entry from download history? The audio file will stay in its folder.",
                "Удалить запись из истории загрузок? Аудиофайл останется в своей папке.",
            )).style(palette.text()).wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, footer.y - inner.y),
        );
        buttons(frame, app, footer, controls, palette);
        return;
    }
    let count = if view.search { app.downloads.results.len() } else { app.downloads.rows.len() };
    view.selected = view.selected.min(count.saturating_sub(1));
    let loading = app.downloads_loading();
    let preparing = app.downloads.preparing.load(Ordering::Relaxed);
    let selected = (!view.search).then(|| app.downloads.rows.get(view.selected)).flatten();
    let active = selected.is_some_and(|r| app.downloads.active.as_deref() == Some(&r.id));
    let error = app.downloads.error.as_ref()
        .or_else(|| selected.and_then(|row| row.error.as_ref()));
    let mut add = ActionGroup {
        title: if view.search { app.text("Music search", "Поиск музыки") } else { app.text("Add music", "Добавить музыку") },
        controls: if view.search {
            vec![control(app.text("New search [F]", "Новый поиск [F]"), A::Search, !loading)]
        } else {
            vec![
                control(app.text("Add link [A]", "Добавить ссылку [A]"), A::New, app.downloads.initialized),
                control(app.text("Find music [F]", "Найти музыку [F]"), A::Search, !loading),
            ]
        },
    };
    let mut record = ActionGroup {
        title: if view.search { app.text("Search result", "Результат поиска") }
            else if selected.is_some() { app.text("Selected entry", "Выбранная запись") }
            else { app.text("Downloads", "Загрузки") },
        controls: Vec::new(),
    };
    if view.search {
        if count > 0 {
            record.controls.push(control(app.text("Download [Enter]", "Скачать [Enter]"), A::Queue, !loading));
        }
        if loading {
            add.controls.push(control(app.text("Cancel search [C]", "Отменить поиск [C]"), A::Cancel, true));
        }
    } else {
        if !app.downloads.initialized || selected.is_some_and(|r| matches!(r.status, Status::Failed | Status::Cancelled)) {
            record.controls.push(control(app.text("Retry [R]", "Повторить [R]"), A::Retry, !loading));
        }
        if selected.is_some_and(|r| r.status != Status::Done) {
            record.controls.push(control(app.text("Edit [E]", "Изменить [E]"), A::Edit, !active));
        }
        if active || selected.is_some_and(|r| r.status == Status::Queued) {
            record.controls.push(control(app.text("Cancel [C]", "Отменить [C]"), A::Cancel, true));
        }
        if selected.is_some_and(|r| r.status == Status::Done) {
            record.controls.push(control(app.text("To desk [Enter]", "На стол [Enter]"), A::Import, true));
        }
    }
    if error.is_some() && (view.search || app.downloads.error.is_none()) {
        record.controls.push(control(app.text("Copy error", "Скопировать ошибку"), A::CopyError, true));
    }
    let mut loader = ActionGroup {
        title: app.text("Downloader", "Загрузчик"),
        controls: if view.search { Vec::new() } else {
            vec![control(
                if app.downloads.config.engine == Engine::YtDlp {
                    "yt-dlp ↔ Cobalt"
                } else {
                    "Cobalt ↔ yt-dlp"
                },
                A::Engine,
                !loading,
            ),
            control(app.text("Settings [S]", "Настройки [S]"), A::Settings, !loading),
            control(
                app.text("Install / update", "Установить / обновить"),
                A::Tools,
                !loading && app.downloads.active.is_none(),
            )]
        },
    };
    if !view.search && loading {
        loader.controls.push(control(app.text("Cancel [C]", "Отменить [C]"), A::Cancel, true));
    }
    if !view.search && app.downloads.error.is_some() {
        loader.controls.push(control(app.text("Copy error", "Скопировать ошибку"), A::CopyError, true));
    }
    let remove = (!view.search && selected.is_some()).then(||
        control(app.text("Remove from history", "Удалить из истории"), A::Remove, !active));
    let close = control(if view.search { app.text("Back [Esc]", "Назад [Esc]") }
        else { app.text("Close [Esc]", "Закрыть [Esc]") }, A::Back, true);
    let close_width = button_width(app, &close.0, &close.1).min(inner.width);
    let remove_width = remove.as_ref().map_or(0, |entry| button_width(app, &entry.0, &entry.1).min(inner.width));
    let gap = u16::from(inner.height >= 30);
    let nav_height = if remove_width > 0 && remove_width + 3 + close_width > inner.width { 2 + gap } else { 1 };
    let columns = inner.width >= 72 && !record.controls.is_empty() && !loader.controls.is_empty();
    let left_width = if columns { (inner.width - 4) / 2 } else { inner.width };
    let right_width = if columns { inner.width - left_width - 4 } else { inner.width };
    let record_height = record.height(app, left_width);
    let loader_height = loader.height(app, right_width);
    let group_gap = if record_height > 0 && loader_height > 0 { gap } else { 0 };
    let group_height = if columns { record_height.max(loader_height) } else { record_height + group_gap + loader_height };
    let footer_height = 1 + group_height + gap + nav_height;
    let footer = Rect::new(inner.x, inner.bottom().saturating_sub(footer_height), inner.width, footer_height);
    let top = Rect::new(inner.x, inner.y + 2 + gap, inner.width, add.height(app, inner.width));
    let body_top = top.bottom() + gap;
    let caption = if view.search {
        format!("{}: {}", app.text("Search", "Поиск"), app.downloads.query)
    } else {
        format!(
            "{} · {}",
            if selected.map_or(app.downloads.config.engine, |job| job.engine) == Engine::Cobalt {
                "Cobalt"
            } else {
                "yt-dlp"
            },
            selected.map_or(&app.downloads.config.directory, |job| &job.folder).display()
        )
    };
    frame.render_widget(
        Paragraph::new(clean(&caption)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let busy = loading || app.downloads.active.is_some();
    let hint = if busy {
        let bytes = app.downloads.progress.load(Ordering::Relaxed);
        let action = if preparing {
            app.text("Preparing downloads…", "Подготовка скачивания…")
        } else {
            app.text("Working…", "Выполняется…")
        };
        if bytes > 0 {
            format!(
                "{} · {:.1} MB",
                action,
                bytes as f64 / 1048576.
            )
        } else {
            format!("{action} {}", app.text("You can close this window.", "Можно закрыть это окно."))
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
    let notice = selected.and_then(|r| r.notice.as_ref());
    let detail = error.or(notice).cloned();
    let error_height = if detail.is_some() {
        3.min(footer.y.saturating_sub(body_top + 1 + gap))
    } else {
        0
    };
    let body = Rect::new(
        inner.x,
        body_top,
        inner.width,
        footer.y.saturating_sub(body_top + error_height + gap),
    );
    view.offset = visible_offset(view.offset, view.selected, body.height as usize, count);
    for (row, index) in (view.offset..count).take(body.height as usize).enumerate() {
        let text = if view.search {
            let hit = &app.downloads.results[index];
            format!("{} — {}", hit.artist, hit.title)
        } else {
            let job = &app.downloads.rows[index];
            let status = match job.status {
                Status::Queued => app.text("Queued", "В очереди"),
                Status::Running if preparing && app.downloads.active.as_deref() == Some(&job.id) => app.text("Preparing", "Подготовка"),
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
    if let Some(detail) = &detail {
        frame.render_widget(
            Paragraph::new(clean(detail))
                .style(palette.text().fg(if error.is_some() { palette.error } else { palette.muted }))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    add.render(frame, app, top, palette);
    frame.render_widget(Paragraph::new("─".repeat(usize::from(footer.width)))
        .style(palette.text().fg(palette.separator)), Rect::new(footer.x, footer.y, footer.width, 1));
    let groups_y = footer.y + 1;
    record.render(frame, app, Rect::new(footer.x, groups_y, left_width, record_height), palette);
    let loader_x = if columns { footer.x + left_width + 4 } else { footer.x };
    let loader_y = if columns { groups_y } else { groups_y + record_height + group_gap };
    loader.render(frame, app, Rect::new(loader_x, loader_y, right_width, loader_height), palette);
    if let Some((label, target, enabled)) = remove {
        button(frame, app, Rect::new(inner.x, inner.bottom() - nav_height, remove_width, 1), &label, target, enabled, palette);
    }
    button(frame, app, Rect::new(inner.right() - close_width, inner.bottom() - 1, close_width, 1), &close.0, close.1, close.2, palette);
}
