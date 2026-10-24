use crate::{
    app::{App, Hit, ServiceAction as A, ServicePage, ServicesView, Target},
    services::Provider,
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
fn action(label: &str, action: A, enabled: bool) -> (String, Target, bool) {
    (label.into(), Target::Services(action), enabled)
}
pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    view: &mut ServicesView,
    area: Rect,
    palette: Palette,
) {
    let inner = file_modal(
        frame,
        app,
        area,
        app.text("Music services", "Музыкальные сервисы").into(),
        48,
        palette,
    );
    if inner.width < 24 || inner.height < 18 {
        return;
    }
    let busy = app.services_loading();
    let selected = app.services.connections.get(view.selected);
    let controls = if view.remove.is_some() {
        vec![
            action(app.text("Cancel [Enter]", "Отмена [Enter]"), A::Back, true),
            action(
                app.text("Remove [D]", "Удалить [D]"),
                A::ConfirmRemove,
                !busy,
            ),
        ]
    } else {
        let mut items = match view.page {
            ServicePage::Connections => vec![
                action("+ Яндекс Музыка", A::Add(Provider::Yandex), !busy),
                action("+ Spotify", A::Add(Provider::Spotify), !busy),
                action("+ YouTube Music", A::Add(Provider::YouTube), !busy),
                action(
                    app.text("Open [Enter]", "Открыть [Enter]"),
                    A::Open,
                    !busy && selected.is_some_and(|c| c.enabled),
                ),
                action(
                    app.text("Sign in", "Войти"),
                    A::Login,
                    !busy && selected.is_some_and(|c| c.enabled),
                ),
                action(
                    app.text("Sign out", "Выйти из аккаунта"),
                    A::Logout,
                    !busy && selected.is_some_and(|c| c.authenticated),
                ),
                action(
                    app.text("Edit [E]", "Изменить [E]"),
                    A::Edit,
                    !busy && selected.is_some(),
                ),
                action(
                    if selected.is_some_and(|c| c.enabled) {
                        app.text("Disable", "Отключить")
                    } else {
                        app.text("Enable", "Включить")
                    },
                    A::Enable,
                    !busy && selected.is_some(),
                ),
                action(
                    app.text("Remove", "Удалить"),
                    A::Remove,
                    !busy && selected.is_some(),
                ),
                action(
                    app.text("Install service support", "Установить поддержку сервисов"),
                    A::Install,
                    !busy,
                ),
                action(
                    app.text("YouTube audio components", "Компоненты аудио YouTube"),
                    A::Tools,
                    !busy,
                ),
            ],
            ServicePage::Tracks => vec![
                action(
                    app.text("Play [Enter]", "Играть [Enter]"),
                    A::Play,
                    !busy
                        && app
                            .services
                            .tracks
                            .get(view.selected)
                            .is_some_and(|t| t.available),
                ),
                action(app.text("Search [F]", "Поиск [F]"), A::Search, !busy),
                action(app.text("My music", "Моя музыка"), A::Library, !busy),
                action(
                    app.text("Liked songs", "Любимые композиции"),
                    A::Liked,
                    !busy,
                ),
                action(app.text("Playlists", "Плейлисты"), A::Playlists, !busy),
                action(
                    app.text("Previous page", "Предыдущие записи"),
                    A::Page(-1),
                    !busy && app.services.page > 0,
                ),
                action(
                    app.text("More recordings", "Еще записи"),
                    A::Page(1),
                    !busy && !app.services.tracks.is_empty(),
                ),
                action(
                    app.text("Spotify devices", "Устройства Spotify"),
                    A::Devices,
                    !busy
                        && app.services.connections.iter().any(|c| {
                            Some(&c.id) == app.services.connection.as_ref()
                                && c.provider == Provider::Spotify
                        }),
                ),
            ],
            ServicePage::Playlists => vec![
                action(
                    app.text("Open [Enter]", "Открыть [Enter]"),
                    A::Open,
                    !busy && !app.services.playlists.is_empty(),
                ),
                action(
                    app.text("Previous page", "Предыдущие плейлисты"),
                    A::Page(-1),
                    !busy && app.services.page > 0,
                ),
                action(
                    app.text("More playlists", "Еще плейлисты"),
                    A::Page(1),
                    !busy && !app.services.playlists.is_empty(),
                ),
                action(app.text("Search", "Поиск"), A::Search, !busy),
            ],
            ServicePage::Devices => vec![
                action(
                    app.text("Choose [Enter]", "Выбрать [Enter]"),
                    A::SelectDevice,
                    app.services
                        .devices
                        .get(view.selected)
                        .is_some_and(|d| d.id.is_some() && !d.is_restricted),
                ),
                action(
                    app.text("Refresh devices", "Обновить устройства"),
                    A::Devices,
                    !busy,
                ),
            ],
        };
        if app.services_playing() {
            items.extend([
                action(app.text("Previous", "Предыдущая"), A::Previous, true),
                action(app.text("Play / pause", "Играть / пауза"), A::Pause, true),
                action(app.text("Stop", "Стоп"), A::Stop, true),
                action(app.text("Next", "Следующая"), A::Next, true),
            ]);
        }
        if app.services.link.lock().is_ok_and(|l| l.is_some()) {
            items.push(action(
                app.text("Open sign-in page", "Открыть страницу входа"),
                A::OpenLink,
                true,
            ));
            items.push(action(
                app.text("Copy link", "Скопировать ссылку"),
                A::CopyLink,
                true,
            ));
        }
        if busy || app.services.login_code.is_some() {
            items.push(action(
                app.text("Cancel operation", "Отменить операцию"),
                A::Cancel,
                true,
            ));
        }
        items.push(action(
            app.text("Connection guide", "Как подключить"),
            A::Guide,
            true,
        ));
        items.push(action(app.text("Back", "Назад"), A::Back, true));
        items
    };
    let footer_height = buttons_height(app, inner.width, &controls)
        .max(2)
        .min(inner.height - 4);
    let footer = Rect::new(
        inner.x,
        inner.bottom() - footer_height,
        inner.width,
        footer_height,
    );
    if let Some(connection) = &view.remove {
        frame.render_widget(
            Paragraph::new(format!(
                "{} «{}»?",
                app.text("Remove connection", "Удалить подключение"),
                clean(&connection.name)
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y, inner.width, footer.y - inner.y),
        );
        buttons(frame, app, footer, controls, palette);
        return;
    }
    let heading = match view.page {
        ServicePage::Connections => app
            .text(
                "Choose a service or add an account.",
                "Выберите сервис или добавьте аккаунт.",
            )
            .to_owned(),
        ServicePage::Tracks => format!(
            "{} · {} · {} {}",
            app.services
                .connections
                .iter()
                .find(|c| Some(&c.id) == app.services.connection.as_ref())
                .map(|c| c.name.as_str())
                .unwrap_or(""),
            app.services
                .query
                .as_deref()
                .unwrap_or(app.text("My music", "Моя музыка")),
            app.text("page", "страница"),
            app.services.page + 1
        ),
        ServicePage::Playlists => app.text("Your playlists", "Ваши плейлисты").into(),
        ServicePage::Devices => app
            .text(
                "Open Spotify on a device, then select it here.",
                "Откройте Spotify на устройстве, затем выберите его здесь.",
            )
            .into(),
    };
    frame.render_widget(
        Paragraph::new(clean(&heading)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let notice = if let Some(code) = &app.services.login_code {
        format!(
            "{}: {code}",
            app.text(
                "Enter this code on the sign-in page",
                "Введите этот код на странице входа"
            )
        )
    } else if busy {
        app.text(
            "Working… You can close this window.",
            "Выполняется… Можно закрыть это окно.",
        )
        .into()
    } else if app.services_playing() {
        app.playback_source_caption()
    } else {
        app.text(
            "Accounts can be renamed, disabled or removed.",
            "Подключения можно переименовать, отключить или удалить.",
        )
        .into()
    };
    frame.render_widget(
        Paragraph::new(clean(&notice)).style(palette.text().fg(palette.muted)),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    let error_height = if app.services.error.is_some() {
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
    let count = match view.page {
        ServicePage::Connections => app.services.connections.len(),
        ServicePage::Tracks => app.services.tracks.len(),
        ServicePage::Devices => app.services.devices.len(),
        ServicePage::Playlists => app.services.playlists.len(),
    };
    view.selected = view.selected.min(count.saturating_sub(1));
    view.offset = visible_offset(view.offset, view.selected, body.height as usize, count);
    for (row, index) in (view.offset..count).take(body.height as usize).enumerate() {
        let text = match view.page {
            ServicePage::Connections => {
                let c = &app.services.connections[index];
                format!(
                    "{} {} · {} · {}",
                    if c.enabled { "[x]" } else { "[ ]" },
                    c.name,
                    c.provider.name(),
                    if c.authenticated {
                        app.text("Signed in", "Вход сохранен")
                    } else {
                        app.text("Not signed in", "Без входа")
                    }
                )
            }
            ServicePage::Tracks => {
                let t = &app.services.tracks[index];
                format!(
                    "{}{} — {} · {}",
                    if t.available { "" } else { "× " },
                    t.artist,
                    t.title,
                    t.album
                )
            }
            ServicePage::Playlists => {
                let p = &app.services.playlists[index];
                format!("{} · {}", p.name, p.count)
            }
            ServicePage::Devices => {
                let d = &app.services.devices[index];
                format!(
                    "{} {}{}",
                    if d.is_active { "▶" } else { " " },
                    d.name,
                    if d.is_restricted {
                        app.text(" · unavailable", " · недоступно")
                    } else {
                        ""
                    }
                )
            }
        };
        let row = Rect::new(body.x, body.y + row as u16, body.width, 1);
        frame.render_widget(
            Paragraph::new(clean(&text)).style(if index == view.selected {
                palette
                    .text()
                    .bg(palette.selection)
                    .fg(palette.selection_text)
            } else {
                palette.text()
            }),
            row,
        );
        app.view.hits.push(Hit {
            area: row,
            target: Target::Services(A::Row(index)),
            enabled: true,
        });
    }
    if count == 0 && !busy {
        frame.render_widget(
            Paragraph::new(match view.page {
                ServicePage::Connections => app.text(
                    "Add Yandex Music, Spotify or YouTube Music below.",
                    "Добавьте Яндекс Музыку, Spotify или YouTube Music кнопками ниже.",
                ),
                ServicePage::Tracks => app.text(
                    "No recordings found. Try another query or open My music.",
                    "Записи не найдены. Измените запрос или откройте Мою музыку.",
                ),
                ServicePage::Playlists => app.text("No playlists found.", "Плейлисты не найдены."),
                ServicePage::Devices => app.text(
                    "No devices found. Open Spotify and refresh this list.",
                    "Устройства не найдены. Откройте Spotify и обновите список.",
                ),
            })
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            body,
        );
    }
    if let Some(error) = &app.services.error {
        frame.render_widget(
            Paragraph::new(clean(error))
                .style(palette.text().fg(palette.error))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, footer.y - error_height, inner.width, error_height),
        );
    }
    buttons(frame, app, footer, controls, palette);
}
