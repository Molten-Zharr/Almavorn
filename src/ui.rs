mod buttons;
mod dialogs;
mod keycaps;
mod library;
mod menus;
mod player;
mod settings;
mod theme;
mod toolbar;
mod visualizer;
mod widgets;
mod workspace;

use self::{dialogs::render_dialog, theme::Palette};
use crate::{
    app::{App, Target},
    input::Action,
    model::{Mode, PlaylistKind},
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let palette = Palette::new(app);
    app.view.hits.clear();
    app.view.keycaps.clear();
    app.view.dialog_scroll_max = 0;
    app.view.waveform_area = Rect::default();
    app.view.artwork_area = Rect::default();
    app.view.progress_area = Rect::default();
    app.view.filter_field = Rect::default();
    app.view.filter_keyboard_area = Rect::default();
    app.view.overlay_area = Rect::default();
    if app
        .view
        .dialog
        .as_ref()
        .is_some_and(crate::app::Dialog::is_settings)
    {
        settings::render_settings(frame, app, palette);
        if !matches!(app.view.dialog, Some(crate::app::Dialog::Settings { .. }))
            && let Some(mut dialog) = app.view.dialog.take()
        {
            app.view.hits.clear();
            app.view.keycaps.clear();
            render_dialog(frame, app, &mut dialog, palette);
            app.view.dialog = Some(dialog);
        }
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(palette.background)),
        area,
    );
    if area.width < 32 || area.height < 18 {
        frame.render_widget(
            Paragraph::new(app.text(
                "Enlarge the window (at least 32 × 18 cells).",
                "Увеличьте окно (не менее 32 × 18 ячеек).",
            ))
            .style(palette.text())
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(toolbar::height(app, area.width)),
        Constraint::Min(9),
        Constraint::Length(1),
    ])
    .split(area);
    toolbar::render(frame, app, parts[0], palette);
    workspace::render(frame, app, parts[1], palette);
    let notice = if app.view.notice_error {
        app.view.notice.clone()
    } else if app.importing() {
        format!(
            "{}: {}",
            app.text("Reading files", "Чтение файлов"),
            app.progress()
        )
    } else if app.busy() {
        app.text("Updating library…", "Обновление библиотеки…")
            .to_owned()
    } else if app
        .view
        .notice_until
        .is_some_and(|until| until > std::time::Instant::now())
    {
        app.view.notice.clone()
    } else {
        context_hint(app, area.width < 120).into()
    };
    let tooltip = app.hovered_target().and_then(|target| match target {
        Target::Action(action) => Some(format!(
            "{}{}",
            buttons::button_shortcut(app, target).map_or(String::new(), |key| format!("{key} · ")),
            if matches!(action, Action::Undo | Action::Redo) {
                format!(
                    "{}. {}",
                    app.history_caption(*action == Action::Redo),
                    app.text(
                        "History lasts until exit; file tags are not restored.",
                        "История до закрытия приложения; теги в файлах не восстанавливаются."
                    )
                )
            } else {
                action.description(app.settings.preferences.language).into()
            }
        )),
        Target::CommandMenu(menu) => Some(menu.title(app.settings.preferences.language).to_owned()),
        _ => None,
    });
    let message = if app.view.notice_error || app.busy() || app.importing() {
        notice
    } else if app.view.layout_editing {
        app.text(
            "Drag a title or border · Ctrl+B / Esc: done",
            "Тяните заголовок или границу · Ctrl+B / Esc: готово",
        )
        .into()
    } else if app.view.toolbar_selected.is_some() {
        app.text(
            "←→: select · Enter: open · Esc: library",
            "←→: выбор · Enter: открыть · Esc: библиотека",
        )
        .into()
    } else {
        tooltip.unwrap_or(notice)
    };
    let help_target = Target::Action(Action::Help);
    let help = app.text("Help", "Справка");
    let help_width = buttons::button_width(app, help, &help_target).min(parts[2].width);
    let navigation = if area.width >= 80 {
        app.text("Tab: panels / menu", "Tab: панели / меню")
    } else {
        ""
    };
    let navigation_width = Span::raw(navigation).width() as u16;
    frame.render_widget(
        Paragraph::new(Line::from(message)).style(
            Style::default()
                .fg(if app.view.notice_error {
                    palette.error
                } else {
                    palette.muted
                })
                .bg(palette.background),
        ),
        Rect::new(
            parts[2].x,
            parts[2].y,
            parts[2]
                .width
                .saturating_sub(help_width + navigation_width + 2),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(navigation).style(palette.text().fg(palette.muted)),
        Rect::new(
            parts[2]
                .right()
                .saturating_sub(help_width + navigation_width + 1),
            parts[2].y,
            navigation_width,
            1,
        ),
    );
    buttons::quiet_button(
        frame,
        app,
        Rect::new(
            parts[2].right().saturating_sub(help_width),
            parts[2].y,
            help_width,
            1,
        ),
        help,
        help_target,
        true,
        palette,
    );
    visualizer::render(frame, app, palette);
    if app.view.filter_keyboard
        && (app.view.dialog.is_none()
            || matches!(app.view.dialog, Some(crate::app::Dialog::Statistics))
                && !app.statistics.view.focused)
    {
        library::filter_keyboard(frame, app, palette);
    }
    if let Some(mut dialog) = app.view.dialog.take() {
        if !matches!(dialog, crate::app::Dialog::Statistics) {
            app.view.hits.clear();
            app.view.keycaps.clear();
        }
        if let crate::app::Dialog::Commands {
            menu,
            selected,
            anchor,
        } = &mut dialog
        {
            menus::render(frame, app, *menu, selected, *anchor, palette);
        } else {
            render_dialog(frame, app, &mut dialog, palette);
        }
        app.view.dialog = Some(dialog);
    }
}

fn context_hint(app: &App, compact: bool) -> &'static str {
    if app.playlist().is_none() {
        return match (app.settings.mode, compact) {
            (Mode::Order, true) => {
                app.text("Order: create a playlist", "Порядок: создайте плейлист")
            }
            (Mode::Chaos, true) => app.text("Chaos: create a playlist", "Хаос: создайте плейлист"),
            (Mode::Order, false) => app.text(
                "Order: create a playlist to collect and keep your music in order.",
                "Порядок: создайте плейлист, чтобы собрать музыку и сохранить порядок композиций.",
            ),
            (Mode::Chaos, false) => app.text(
                "Chaos: create a playlist for a freely editable music collection.",
                "Хаос: создайте плейлист для свободной подборки музыки.",
            ),
        };
    }
    if !app.library.query.is_empty() {
        return if compact {
            app.text("Filter: this playlist", "Фильтр: внутри плейлиста")
        } else {
            app.text(
                "Filter: matching tracks in this playlist. Clear the filter to see all tracks.",
                "Фильтр: совпадения в текущем плейлисте. Очистите строку, чтобы вернуть все композиции.",
            )
        };
    }
    if app.view.focus == crate::app::Focus::Playlists {
        return match (app.settings.mode, compact) {
            (Mode::Order, true) => app.text("Order: manage playlists", "Порядок: работа с плейлистами"),
            (Mode::Chaos, true) => app.text("Chaos: manage playlists", "Хаос: работа с плейлистами"),
            (Mode::Order, false) => app.text(
                "Order: choose, create and arrange saved playlists. Edit unlocks changes.",
                "Порядок: кнопка Редактировать открывает только выбранный плейлист. Готово снова защищает его.",
            ),
            (Mode::Chaos, false) => app.text(
                "Chaos: choose, create and freely rearrange your playlists.",
                "Хаос: выбирайте, создавайте и свободно переставляйте плейлисты.",
            ),
        };
    }
    if app
        .playlist()
        .is_some_and(|playlist| playlist.kind == PlaylistKind::SortingDesk)
    {
        return if compact {
            app.text("Sorting desk: organize music", "Сортировочный стол: разбор")
        } else {
            app.text(
                "Shared desk: organize music and copy it to playlists. Undo lasts until exit.",
                "Общий стол: разбирайте музыку и добавляйте ее в плейлисты. Отмена доступна до закрытия приложения.",
            )
        };
    }
    if let Some(playlist) = app.playlist()
        && playlist.entries.is_empty()
    {
        return if app.playlist_editable(playlist) {
            app.text(
                "Empty playlist: add music",
                "Плейлист пуст: добавьте музыку",
            )
        } else {
            app.text(
                "Empty playlist: enable Edit",
                "Плейлист пуст: включите Правку",
            )
        };
    }
    match (app.settings.mode, app.selected_playlist_unlocked(), compact) {
        (Mode::Chaos, _, true) => app.text("Chaos: a free collection", "Хаос: свободная подборка"),
        (Mode::Chaos, _, false) => app.text(
            "Chaos: build mixes. Undo history lasts until you close the app.",
            "Хаос: собирайте подборки. История отмены хранится до закрытия приложения.",
        ),
        (Mode::Order, true, true) => app.text("Order: editing playlists", "Порядок: правка плейлиста"),
        (Mode::Order, true, false) => app.text(
            "Order · Edit: add, mark, rearrange and remove tracks from playlists.",
            "Порядок · Правка: добавляйте, отмечайте, переставляйте и убирайте композиции из плейлистов.",
        ),
        (Mode::Order, false, true) => app.text("Order: saved playlists", "Порядок: сохраненные плейлисты"),
        (Mode::Order, false, false) => app.text(
            "Order: listen to saved playlists. Enable Edit to change their contents.",
            "Порядок: слушайте сохраненные плейлисты. Для изменения состава включите Правку.",
        ),
    }
}
