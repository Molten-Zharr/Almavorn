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
    render_content(frame, app, view, inner, palette);
}

pub(in crate::ui) fn render_content(
    frame: &mut Frame,
    app: &mut App,
    view: &mut ServicesView,
    inner: Rect,
    palette: Palette,
) {
    if inner.width < 24 || inner.height < 18 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window to manage music services.",
                "Увеличьте окно для управления музыкальными сервисами.",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let busy = app.services_loading();
    let selected = app.services.connections.get(view.selected);
    let selected_spotify = selected.is_some_and(|c| c.provider == Provider::Spotify);
    let connection = app
        .services
        .connections
        .iter()
        .find(|c| Some(&c.id) == app.services.connection.as_ref() && c.enabled);
    let has_connection = connection.is_some();
    let ready = !busy && has_connection;
    let spotify = connection.is_some_and(|c| c.provider == Provider::Spotify);
    let mut controls = if view.remove.is_some() {
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
                action(
                    if selected_spotify {
                        if selected.is_some_and(|c| c.authenticated) {
                            app.text("My music [Enter]", "Моя музыка [Enter]")
                        } else { app.text("Connect [Enter]", "Подключить [Enter]") }
                    } else { app.text("Open [Enter]", "Открыть [Enter]") },
                    A::Open,
                    !busy && selected.is_some_and(|c| c.enabled),
                ),
                action(
                    if selected_spotify && selected.is_some_and(|c| c.authenticated) {
                        app.text("Sign in again", "Повторить вход")
                    } else { app.text("Sign in", "Войти") },
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
                    ready
                        && app
                            .services
                            .tracks
                            .get(view.selected)
                            .is_some_and(|t| t.available),
                ),
                action(app.text("Search [F]", "Поиск [F]"), A::Search, ready),
                action(app.text("My music", "Моя музыка"), A::Library, ready),
                action(
                    app.text("Liked songs", "Любимые композиции"),
                    A::Liked,
                    ready,
                ),
                action(app.text("Playlists", "Плейлисты"), A::Playlists, ready),
                action(
                    app.text("Previous page", "Предыдущие записи"),
                    A::Page(-1),
                    ready && app.services.page > 0,
                ),
                action(
                    app.text("More recordings", "Еще записи"),
                    A::Page(1),
                    ready && !app.services.tracks.is_empty(),
                ),
                action(
                    app.text("Spotify devices", "Устройства Spotify"),
                    A::Devices,
                    ready && spotify,
                ),
            ],
            ServicePage::Playlists => vec![
                action(
                    app.text("Refresh playlists", "Обновить плейлисты"),
                    A::Playlists,
                    ready,
                ),
                action(
                    app.text("Open [Enter]", "Открыть [Enter]"),
                    A::Open,
                    ready && !app.services.playlists.is_empty(),
                ),
                action(
                    app.text("Previous page", "Предыдущие плейлисты"),
                    A::Page(-1),
                    ready && app.services.page > 0,
                ),
                action(
                    app.text("More playlists", "Еще плейлисты"),
                    A::Page(1),
                    ready && !app.services.playlists.is_empty(),
                ),
                action(app.text("Search", "Поиск"), A::Search, ready),
            ],
            ServicePage::Devices => vec![
                action(
                    app.text("Choose [Enter]", "Выбрать [Enter]"),
                    A::SelectDevice,
                    ready
                        && spotify
                        && app
                            .services
                            .devices
                            .get(view.selected)
                            .is_some_and(|d| d.id.is_some() && !d.is_restricted),
                ),
                action(
                    app.text("Refresh devices", "Обновить устройства"),
                    A::Devices,
                    ready && spotify,
                ),
            ],
        };
        if view.page == ServicePage::Connections && selected_spotify {
            if !selected.is_some_and(|c| c.authenticated) {
                items.retain(|(_, target, _)| !matches!(target, Target::Services(A::Login)));
            }
            if selected.is_some_and(|c| c.authenticated) {
                items.insert(1, action(app.text("Choose device", "Выбрать устройство"), A::Devices,
                    !busy && selected.is_some_and(|c| c.enabled)));
            }
        }
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
        items
    };
    if view.remove.is_none() && view.page != ServicePage::Connections {
        controls.insert(
            0,
            (
                app.text("Connections", "Подключения").into(),
                Target::SettingsTab(crate::app::SettingsPage::Services),
                true,
            ),
        );
    }
    let mut footer_height = buttons_height(app, inner.width, &controls)
        .max(2)
        .min(inner.height - 4);
    let list_add = view.remove.is_none()
        && view.page == ServicePage::Connections
        && inner.height.saturating_sub(footer_height + 3
            + if app.services.error.is_some() { 3 } else { 0 }) >= 5;
    if view.remove.is_none() && view.page == ServicePage::Connections && !list_add {
        controls.splice(0..0, [
            action("+ Яндекс Музыка", A::Add(Provider::Yandex), !busy),
            action("+ Spotify", A::Add(Provider::Spotify), !busy),
            action("+ YouTube Music", A::Add(Provider::YouTube), !busy),
        ]);
        footer_height = buttons_height(app, inner.width, &controls)
            .max(2)
            .min(inner.height - 4);
    }
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
    } else if busy && app.services.link.lock().is_ok_and(|link| link.is_some()) {
        app.text("Confirm access in your browser. If it did not open, use Open sign-in page below.",
            "Подтвердите доступ в браузере. Если он не открылся, нажмите Открыть страницу входа внизу.").into()
    } else if busy {
        app.text(
            "Working… You can close this window.",
            "Выполняется… Можно закрыть это окно.",
        )
        .into()
    } else if view.page == ServicePage::Connections && selected_spotify {
        if selected.is_some_and(|c| c.authenticated) {
            app.text("Account connected. Open Spotify on your device, then click Choose device.",
                "Аккаунт подключен. Откройте Spotify на устройстве и нажмите Выбрать устройство.")
        } else {
            app.text("Click Connect. The player will guide you through the first sign-in.",
                "Нажмите Подключить. Плеер проведет через первый вход.")
        }.into()
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
        Paragraph::new(clean(&notice)).style(palette.text().fg(palette.muted)).wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y + 1, inner.width, 2),
    );
    let error_height = if app.services.error.is_some() {
        3.min(footer.y.saturating_sub(inner.y + 3))
    } else {
        0
    };
    let mut body_y = inner.y + 3;
    if list_add {
        let available = footer.y.saturating_sub(body_y + error_height);
        if available >= 5 {
            frame.render_widget(
                Paragraph::new(app.text("Add a service", "Добавить сервис"))
                    .style(palette.text().fg(palette.accent)),
                Rect::new(inner.x, body_y, inner.width, 1),
            );
            body_y += 1;
            for entry in [
                action("+ Яндекс Музыка", A::Add(Provider::Yandex), !busy),
                action("+ Spotify", A::Add(Provider::Spotify), !busy),
                action("+ YouTube Music", A::Add(Provider::YouTube), !busy),
            ] {
                buttons(frame, app, Rect::new(inner.x, body_y, inner.width, 1), vec![entry], palette);
                body_y += 1;
            }
            body_y += 1;
        }
    }
    let body = Rect::new(
        inner.x,
        body_y,
        inner.width,
        footer.y.saturating_sub(body_y + error_height),
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
            Paragraph::new(
                if view.page != ServicePage::Connections && !has_connection {
                    app.text(
                        "Choose an enabled account in Connections.",
                        "Выберите включенное подключение в разделе Подключения.",
                    )
                } else if view.page == ServicePage::Devices && !spotify {
                    app.text(
                        "Device selection is available for Spotify. Choose Spotify in Connections.",
                        "Устройства доступны для Spotify. Выберите его в разделе Подключения.",
                    )
                } else {
                    match view.page {
                        ServicePage::Connections => app.text(
                            "No accounts connected yet.",
                            "Пока нет подключенных аккаунтов.",
                        ),
                        ServicePage::Tracks => app.text(
                            "No recordings found. Try another query or open My music.",
                            "Записи не найдены. Измените запрос или откройте Мою музыку.",
                        ),
                        ServicePage::Playlists => app.text(
                            "Press Refresh playlists to load this account's playlists.",
                            "Нажмите Обновить плейлисты, чтобы загрузить их из аккаунта.",
                        ),
                        ServicePage::Devices => app.text(
                            "No devices found. Open Spotify and refresh this list.",
                            "Устройства не найдены. Откройте Spotify и обновите список.",
                        ),
                    }
                },
            )
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
