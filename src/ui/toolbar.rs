use super::{
    theme::Palette,
    widgets::{block, button, button_rows, button_width},
};
use crate::{
    app::{App, Target},
    input::Action,
    model::Mode,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Borders,
};

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

fn layout(app: &App, groups: &[Group], width: u16, compact: bool) -> (Vec<Rect>, u16) {
    let mut rects = Vec::with_capacity(groups.len());
    let (mut x, mut y, mut row_height) = (0u16, 0u16, 0u16);
    let border = if compact { 0 } else { 2 };
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
        let height = rows.saturating_mul(if compact { 1 } else { 3 }) + if compact { 1 } else { 2 };
        if x > 0 && x.saturating_add(group_width) > width {
            x = 0;
            y = y
                .saturating_add(row_height)
                .saturating_add(u16::from(!compact));
            row_height = 0;
        }
        rects.push(Rect::new(x, y, group_width, height));
        row_height = row_height.max(height);
        x = x.saturating_add(group_width).saturating_add(1);
    }
    (rects, y.saturating_add(row_height))
}

pub(super) fn height(app: &App, groups: &[Group], width: u16, compact: bool) -> u16 {
    layout(app, groups, width, compact).1
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    groups: Vec<Group>,
    compact: bool,
    palette: Palette,
) {
    let (rects, _) = layout(app, &groups, area.width, compact);
    for (group, local) in groups.into_iter().zip(rects) {
        let rect = Rect::new(
            area.x + local.x,
            area.y + local.y,
            local.width,
            local.height,
        )
        .intersection(area);
        let mut outer = block(group.title, palette, false).title_style(
            Style::default()
                .fg(palette.help_heading)
                .add_modifier(Modifier::BOLD),
        );
        if compact {
            outer = outer.borders(Borders::TOP);
        }
        let inner = outer.inner(rect);
        frame.render_widget(outer, rect);
        let (mut x, mut y) = (inner.x, inner.y);
        let key_height = if compact { 1 } else { 3 };
        for (label, target, enabled) in group.entries {
            let width = button_width(app, &label, &target).min(inner.width);
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
                &label,
                target,
                enabled,
                palette,
            );
            x = x.saturating_add(width).saturating_add(1);
        }
    }
}
