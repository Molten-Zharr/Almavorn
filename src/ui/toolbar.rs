use super::{theme::Palette, widgets::button_shortcut};
use crate::{
    app::{App, Hit, Target},
    input::Action,
    model::Mode,
    workspace::{Control, Panel},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier},
    text::{Line, Span},
    widgets::Paragraph,
};

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

pub(super) fn command_width(app: &App, label: &str, target: &Target) -> u16 {
    (Span::raw(label).width()
        + button_shortcut(app, target).map_or(0, |key| Span::raw(key).width() + 3)
        + 2)
    .min(usize::from(u16::MAX)) as u16
}

fn band(target: &Target) -> usize {
    match target {
        Target::Mode(_) => 0,
        Target::Action(action) => match action {
            Action::ToggleDesk => 1,
            Action::ToggleEdit => 2,
            Action::Settings | Action::Help | Action::Quit => 2,
            Action::Undo | Action::Redo => 1,
            Action::Transfer | Action::MoveUp | Action::MoveDown => 1,
            Action::Search | Action::Sort | Action::Metadata | Action::Mark => 2,
            Action::VolumeDown | Action::VolumeUp => 1,
            _ => 0,
        },
        _ => 0,
    }
}

fn cells(app: &App, width: u16, group: &Group) -> Vec<(usize, Rect)> {
    if width == 0 {
        return Vec::new();
    }
    let margin = u16::from(width >= 36);
    let available = width.saturating_sub(margin * 2);
    let gap = 2u16;
    let separation = u16::from(app.workspace.bounds.width >= 80);
    let mut bands: [Vec<usize>; 3] = Default::default();
    for (index, (_, target, _)) in group.entries.iter().enumerate() {
        bands[band(target)].push(index);
    }
    let measure = |index: usize| {
        let (label, target, _) = &group.entries[index];
        command_width(app, label, target).min(available)
    };
    let needed = |indices: &[usize]| {
        indices.iter().map(|index| measure(*index)).sum::<u16>()
            + gap * indices.len().saturating_sub(1) as u16
    };
    let mut result = Vec::with_capacity(group.entries.len());
    let mut place = |indices: &[usize], y: u16, left: u16, space: u16| {
        let padding = (space.saturating_sub(needed(indices)) / indices.len().max(1) as u16).min(2);
        let occupied = needed(indices) + padding * indices.len() as u16;
        let mut x = left + space.saturating_sub(occupied) / 2;
        for index in indices {
            let size = measure(*index).saturating_add(padding);
            result.push((*index, Rect::new(x, y, size, 1)));
            x = x.saturating_add(size).saturating_add(gap);
        }
    };
    if group.panel == Panel::Player {
        let left = needed(&bands[0]);
        let right = needed(&bands[1]);
        if left.saturating_add(right).saturating_add(gap * 2) <= available {
            let padding = (available.saturating_sub(left + right + gap * 2) / 2).min(8);
            place(&bands[0], 0, margin, left + padding);
            place(
                &bands[1],
                0,
                width.saturating_sub(margin + right + padding),
                right + padding,
            );
            return result;
        }
    }
    let mut widths: Vec<_> = (0..group.entries.len()).map(measure).collect();
    widths.sort_unstable();
    let median = widths.get(widths.len() / 2).copied().unwrap_or(1);
    let preferred = match group.panel {
        Panel::Application => 2,
        Panel::Player => 4,
        _ => 3,
    };
    let columns = (available.saturating_add(gap) / median.saturating_add(gap))
        .max(1)
        .min(preferred);
    let cell_width =
        (available.saturating_sub(gap * (columns - 1)) / columns).min(median.saturating_add(4));
    let occupied = cell_width * columns + gap * (columns - 1);
    let left = margin + available.saturating_sub(occupied) / 2;
    let mut y = 0u16;
    for indices in bands.iter().filter(|indices| !indices.is_empty()) {
        if y > 0 {
            y = y.saturating_add(separation);
        }
        let mut column = 0u16;
        for index in indices {
            let span = measure(*index)
                .saturating_add(gap)
                .div_ceil(cell_width + gap)
                .max(1)
                .min(columns);
            if column + span > columns {
                y = y.saturating_add(1);
                column = 0;
            }
            let size = if span == columns && measure(*index) > occupied {
                available
            } else {
                cell_width * span + gap * (span - 1)
            };
            let x = if size > occupied {
                margin
            } else {
                left + column * (cell_width + gap)
            };
            result.push((*index, Rect::new(x, y, size, 1)));
            column += span;
        }
        y = y.saturating_add(1);
    }
    result
}

pub(super) fn rows(app: &App, width: u16, group: &Group) -> u16 {
    cells(app, width, group)
        .iter()
        .map(|(_, rect)| rect.bottom())
        .max()
        .unwrap_or(0)
}

type Command = (Action, &'static str, &'static str);
type Section = (Panel, &'static str, &'static str, &'static [Command]);

pub(super) fn commands(app: &App) -> Vec<Group> {
    let mut modes: Vec<_> = Mode::ALL
        .into_iter()
        .map(|mode| {
            (
                mode.name(app.settings.language).into(),
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
            format!("[{}] {}", if checked { "x" } else { " " }, app.text(en, ru)),
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
                .map(|(label, target, _)| command_width(app, label, target) + border + 2)
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
                used.saturating_add(command_width(app, label, target))
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
        let rows = rows(app, group_width.saturating_sub(border), group);
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
    for (index, cell) in cells(app, inner.width, group) {
        if cell.bottom() > inner.height {
            break;
        }
        let (label, target, enabled) = &group.entries[index];
        command_button(
            frame,
            app,
            Rect::new(inner.x + cell.x, inner.y + cell.y, cell.width, cell.height),
            label,
            target.clone(),
            *enabled,
            palette,
        );
    }
}

fn command_button(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    let active = match target {
        Target::Mode(mode) => app.settings.mode == mode,
        Target::Action(Action::ToggleEdit) => app.editing,
        Target::Action(Action::ToggleDesk) => app.settings.sorting_desk,
        _ => false,
    };
    let hovered = enabled && app.hovered(area);
    let surface = match (palette.background, palette.selection) {
        (Color::Rgb(r, g, b), Color::Rgb(sr, sg, sb)) => {
            let blend = |background: u8, selection: u8| {
                ((u16::from(background) * 4 + u16::from(selection)) / 5) as u8
            };
            Color::Rgb(blend(r, sr), blend(g, sg), blend(b, sb))
        }
        (background, _) => background,
    };
    let mut style = palette.text().bg(surface);
    if active || hovered {
        style = style.bg(palette.selection).add_modifier(Modifier::BOLD);
    }
    if !enabled {
        style = style.fg(palette.muted);
    }
    let mut content = vec![Span::styled(
        if active || hovered { "│" } else { " " },
        style.fg(palette.accent),
    )];
    if let Some(shortcut) = button_shortcut(app, &target) {
        let key_style = style.bg(palette.selection).fg(if enabled {
            palette.button_text
        } else {
            palette.muted
        });
        content.push(Span::styled("[", key_style.fg(palette.muted)));
        content.push(Span::styled(
            shortcut,
            key_style.add_modifier(Modifier::BOLD),
        ));
        content.push(Span::styled("]", key_style.fg(palette.muted)));
        content.push(Span::styled(" ", style));
    }
    content.push(Span::styled(label.to_owned(), style));
    frame.render_widget(Paragraph::new(Line::from(content)).style(style), area);
    app.hits.push(Hit {
        area,
        target,
        enabled,
    });
}
