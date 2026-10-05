use super::{
    theme::Palette,
    widgets::{button, button_rows, button_width},
};
use crate::{
    app::{App, Target},
    input::Action,
    model::Mode,
};
use ratatui::{Frame, layout::Rect};

pub(super) struct Group {
    pub title: String,
    pub entries: Vec<(String, Target, bool)>,
}

type Command = (Action, &'static str, &'static str);
type Section = (&'static str, &'static str, &'static [Command]);

pub(super) fn commands(app: &App) -> Vec<Group> {
    let mut modes: Vec<_> = Mode::ALL
        .into_iter()
        .map(|mode| {
            (
                format!(
                    "{}{}",
                    if app.settings.mode == mode { "* " } else { "" },
                    mode.name(app.settings.language)
                ),
                Target::Mode(mode),
                !app.busy(),
            )
        })
        .collect();
    for (action, en, ru, checked) in [
        (Action::ToggleEdit, "Edit", "Правка", app.editing),
        (
            Action::ToggleDesk,
            "Sorting desk",
            "Сортировочный стол",
            app.settings.sorting_desk,
        ),
    ] {
        modes.push((
            format!("{} {}", if checked { "x" } else { " " }, app.text(en, ru)),
            Target::Action(action),
            app.allowed(action),
        ));
    }
    let mut groups = vec![Group {
        title: app.text("Mode", "Режим").into(),
        entries: modes,
    }];
    let sections: &[Section] = &[
        (
            "Add music",
            "Добавление",
            &[
                (Action::AddFiles, "Files", "Файлы"),
                (Action::AddFolder, "Folder", "Папка"),
                (Action::AddNew, "New files", "Новые файлы"),
            ],
        ),
        (
            "Playlists",
            "Плейлисты",
            &[
                (Action::NewPlaylist, "+ Playlist", "+ Плейлист"),
                (Action::Rename, "Rename", "Имя"),
                (Action::Delete, "Remove", "Убрать"),
                (Action::Transfer, "Copy to…", "Копировать…"),
                (Action::MoveUp, "Move up", "Выше"),
                (Action::MoveDown, "Move down", "Ниже"),
            ],
        ),
        (
            "View",
            "Просмотр",
            &[
                (Action::Search, "Search", "Поиск"),
                (Action::Sort, "Sort", "Сортировка"),
                (Action::Metadata, "Info", "Сведения"),
                (Action::Mark, "Mark", "Отметить"),
            ],
        ),
        (
            "History",
            "История",
            &[
                (Action::Undo, "Undo", "Отмена"),
                (Action::Redo, "Redo", "Повтор"),
            ],
        ),
        (
            "Application",
            "Приложение",
            &[
                (Action::Settings, "Settings", "Настройки"),
                (Action::Help, "Help", "Помощь"),
                (Action::Quit, "Exit", "Выход"),
            ],
        ),
    ];
    groups.extend(sections.iter().map(|(en, ru, actions)| {
        Group {
            title: app.text(en, ru).into(),
            entries: actions
                .iter()
                .map(|(action, en, ru)| {
                    (
                        app.text(en, ru).into(),
                        Target::Action(*action),
                        app.allowed(*action),
                    )
                })
                .collect(),
        }
    }));
    groups
}

pub(super) fn layout(app: &App, groups: &[Group], width: u16, compact: bool) -> (Vec<Rect>, u16) {
    let mut rects = Vec::with_capacity(groups.len());
    let (mut x, mut y, mut row_height) = (0u16, 0u16, 0u16);
    let border = 2;
    for group in groups {
        let preferred = group
            .entries
            .iter()
            .fold(0u16, |used, (label, target, _)| {
                used.saturating_add(button_width(app, label, target))
                    .saturating_add(1)
            })
            .saturating_sub(1)
            .saturating_add(border);
        let group_width = preferred
            .max(group.title.chars().count() as u16 + 2)
            .min(width);
        let rows = button_rows(app, group_width.saturating_sub(border), &group.entries);
        let height = rows + if compact { 1 } else { 2 };
        if x > 0 && x.saturating_add(group_width) > width {
            x = 0;
            y = y.saturating_add(row_height);
            row_height = 0;
        }
        rects.push(Rect::new(x, y, group_width, height));
        row_height = row_height.max(height);
        x = x.saturating_add(group_width).saturating_add(1);
    }
    (rects, y.saturating_add(row_height))
}

pub(super) fn contents(
    frame: &mut Frame,
    app: &mut App,
    inner: Rect,
    group: &Group,
    palette: Palette,
) {
    let (mut x, mut y) = (inner.x, inner.y);
    let key_height = 1;
    for (label, target, enabled) in &group.entries {
        let width = button_width(app, label, target).min(inner.width);
        if x > inner.x && x.saturating_add(width) > inner.right() {
            x = inner.x;
            y = y.saturating_add(key_height);
        }
        if y.saturating_add(key_height) > inner.bottom() {
            break;
        }
        button(
            frame,
            app,
            Rect::new(x, y, width, key_height),
            label,
            target.clone(),
            *enabled,
            palette,
        );
        x = x.saturating_add(width).saturating_add(1);
    }
}
