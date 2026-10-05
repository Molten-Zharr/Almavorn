use super::{
    theme::Palette,
    widgets::{button, button_rows, button_width},
};
use crate::{
    app::{App, Target},
    input::Action,
    model::Mode,
    workspace::{Control, Panel},
};
use ratatui::{Frame, layout::Rect};

pub(super) struct Group {
    pub panel: Panel,
    pub title: String,
    pub entries: Vec<(String, Target, bool)>,
}

impl Group {
    pub(super) fn retain_visible(&mut self, app: &App) {
        self.entries.retain(|(_, target, _)| {
            let control = match target {
                Target::Action(action) => Some(Control::Action(*action)),
                Target::Mode(mode) => Some(Control::Mode(*mode)),
                _ => None,
            };
            control.is_none_or(|control| !app.settings.workspace.hidden_controls.contains(&control))
        });
    }
}

type Command = (Action, &'static str, &'static str);
type Section = (Panel, &'static str, &'static str, &'static [Command]);

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
        panel: Panel::Application,
        title: app.text("Application", "Приложение").into(),
        entries: modes,
    }];
    groups[0].entries.extend(
        [
            (Action::Settings, "Settings", "Настройки"),
            (Action::Help, "Help", "Помощь"),
            (Action::Quit, "Exit", "Выход"),
        ]
        .map(|(action, en, ru)| {
            (
                app.text(en, ru).into(),
                Target::Action(action),
                app.allowed(action),
            )
        }),
    );
    let sections: &[Section] = &[
        (
            Panel::Add,
            "Add music",
            "Добавление",
            &[
                (Action::AddFiles, "Files", "Файлы"),
                (Action::AddFolder, "Folder", "Папка"),
                (Action::AddNew, "New files", "Новые файлы"),
                (Action::Undo, "Undo", "Отмена"),
                (Action::Redo, "Redo", "Повтор"),
            ],
        ),
        (
            Panel::PlaylistActions,
            "Playlist commands",
            "Команды плейлистов",
            &[
                (Action::NewPlaylist, "+ Playlist", "+ Плейлист"),
                (Action::Rename, "Rename", "Имя"),
                (Action::Delete, "Remove", "Убрать"),
                (Action::Transfer, "Copy to…", "Копировать…"),
                (Action::MoveUp, "Move up", "Выше"),
                (Action::MoveDown, "Move down", "Ниже"),
                (Action::Search, "Search", "Поиск"),
                (Action::Sort, "Sort", "Сортировка"),
                (Action::Metadata, "Info", "Сведения"),
                (Action::Mark, "Mark", "Отметить"),
            ],
        ),
    ];
    groups.extend(sections.iter().map(|(panel, en, ru, actions)| {
        Group {
            panel: *panel,
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
    for group in &mut groups {
        group.retain_visible(app);
    }
    groups
}

pub(super) fn layout(app: &App, groups: &[Group], width: u16, compact: bool) -> (Vec<Rect>, u16) {
    let mut rects = Vec::with_capacity(groups.len());
    let (mut x, mut y, mut row_height) = (0u16, 0u16, 0u16);
    let border = 2;
    let minima: Vec<u16> = groups
        .iter()
        .map(|group| {
            group
                .entries
                .iter()
                .map(|(label, target, _)| button_width(app, label, target) + border)
                .max()
                .unwrap_or(20)
                .max(group.title.chars().count() as u16 + 13)
        })
        .collect();
    let visible_count = groups
        .iter()
        .filter(|group| !app.settings.workspace.hidden.contains(&group.panel))
        .count();
    let minimum_total = groups
        .iter()
        .zip(&minima)
        .filter(|(group, _)| !app.settings.workspace.hidden.contains(&group.panel))
        .map(|(_, minimum)| *minimum)
        .sum::<u16>()
        .saturating_add(visible_count.saturating_sub(1) as u16);
    let distribute = minimum_total <= width;
    let spare = width.saturating_sub(minimum_total);
    let count = visible_count.max(1) as u16;
    let mut visible_index = 0;
    for (index, group) in groups.iter().enumerate() {
        if app.settings.workspace.hidden.contains(&group.panel) {
            rects.push(Rect::new(x, y, 0, 0));
            continue;
        }
        let preferred = group
            .entries
            .iter()
            .fold(0u16, |used, (label, target, _)| {
                used.saturating_add(button_width(app, label, target))
                    .saturating_add(1)
            })
            .saturating_sub(1)
            .saturating_add(border);
        let available = if distribute {
            minima[index] + spare / count + u16::from(visible_index < spare % count)
        } else {
            width
        };
        visible_index += 1;
        let group_width = if distribute {
            available
        } else {
            preferred.max(minima[index]).min(available)
        };
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
