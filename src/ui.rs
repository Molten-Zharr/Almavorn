use crate::{
    app::{App, Browser, Dialog, Focus, Hit, Target, TextDialog, TextPurpose},
    input::Action,
    model::{Language, Mode, Placement, duration_text},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Row, Table, Wrap},
};

#[derive(Clone, Copy)]
struct Palette {
    background: Color,
    panel: Color,
    text: Color,
    muted: Color,
    accent: Color,
    selection: Color,
    error: Color,
}

impl Palette {
    fn new(app: &App) -> Self {
        let theme = &app.settings.themes[app.settings.mode.index()];
        let [r, g, b] = theme.accent;
        if theme.light {
            Self {
                background: Color::Rgb(239, 242, 246),
                panel: Color::Rgb(250, 251, 253),
                text: Color::Rgb(31, 39, 49),
                muted: Color::Rgb(85, 99, 116),
                accent: Color::Rgb(
                    r.saturating_sub(80),
                    g.saturating_sub(80),
                    b.saturating_sub(80),
                ),
                selection: Color::Rgb(212, 225, 239),
                error: Color::Rgb(174, 37, 57),
            }
        } else {
            Self {
                background: Color::Rgb(18, 22, 29),
                panel: Color::Rgb(25, 31, 41),
                text: Color::Rgb(223, 228, 238),
                muted: Color::Rgb(138, 153, 175),
                accent: Color::Rgb(r, g, b),
                selection: Color::Rgb(49, 61, 78),
                error: Color::Rgb(255, 119, 135),
            }
        }
    }
    fn text(self) -> Style {
        Style::default().fg(self.text).bg(self.panel)
    }
}

fn block(title: String, palette: Palette, active: bool) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if active {
            palette.accent
        } else {
            palette.muted
        }))
        .style(palette.text())
}

fn button(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    label: &str,
    target: Target,
    enabled: bool,
    palette: Palette,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut style = if enabled {
        palette
            .text()
            .fg(palette.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        palette.text().fg(palette.muted)
    };
    if enabled && app.hovered(area) {
        style = style.bg(palette.selection);
    }
    frame.render_widget(Paragraph::new(format!("[{label}] ")).style(style), area);
    app.hits.push(Hit {
        area,
        target,
        enabled,
    });
}

fn buttons(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    entries: Vec<(String, Target, bool)>,
    palette: Palette,
) -> u16 {
    let mut x = area.x;
    let mut y = area.y;
    for (label, target, enabled) in entries {
        let width = (label.chars().count() as u16 + 3).min(area.width);
        if x > area.x && x + width > area.right() {
            x = area.x;
            y += 1;
        }
        if y >= area.bottom() {
            break;
        }
        button(
            frame,
            app,
            Rect::new(x, y, width, 1),
            &label,
            target,
            enabled,
            palette,
        );
        x += width;
    }
    y.saturating_sub(area.y) + 1
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let palette = Palette::new(app);
    app.hits.clear();
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
    let toolbar_height = if area.width < 70 { 5 } else { 3 };
    let header_height = if area.width < 70 { 3 } else { 2 };
    let footer_height = if area.width < 80 { 4 } else { 3 };
    let parts = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Length(toolbar_height),
        Constraint::Min(9),
        Constraint::Length(footer_height),
    ])
    .split(area);
    let header = Line::from(vec![
        Span::styled(
            " ALMAVORN ",
            Style::default()
                .fg(palette.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" · {}", app.text("Music player", "Музыкальный плеер")),
            Style::default().fg(palette.muted),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(header),
        Rect::new(parts[0].x, parts[0].y, parts[0].width, 1),
    );
    let lang = app.settings.language;
    let mut modes: Vec<_> = Mode::ALL
        .into_iter()
        .map(|mode| {
            (
                format!(
                    "{}{}",
                    if app.settings.mode == mode { "* " } else { "" },
                    mode.name(lang)
                ),
                Target::Mode(mode),
                !app.busy(),
            )
        })
        .collect();
    modes.push((
        format!(
            "{} {}",
            if app.editing { "x" } else { " " },
            app.text("Edit", "Правка")
        ),
        Target::Action(Action::ToggleEdit),
        app.allowed(Action::ToggleEdit),
    ));
    modes.push((
        format!(
            "{} {}",
            if app.settings.sorting_desk { "x" } else { " " },
            app.text("Sorting desk", "Сортировочный стол")
        ),
        Target::Action(Action::ToggleDesk),
        !app.busy(),
    ));
    buttons(
        frame,
        app,
        Rect::new(
            parts[0].x,
            parts[0].y + 1,
            parts[0].width,
            header_height - 1,
        ),
        modes,
        palette,
    );
    let commands = [
        (Action::AddFiles, "Files", "Файлы"),
        (Action::AddFolder, "Folder", "Папка"),
        (Action::AddNew, "New files", "Новые файлы"),
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
        (Action::Undo, "Undo", "Отмена"),
        (Action::Redo, "Redo", "Повтор"),
        (Action::Settings, "Settings", "Настройки"),
        (Action::Help, "Help", "Помощь"),
        (Action::Quit, "Exit", "Выход"),
    ];
    let entries = commands
        .into_iter()
        .map(|(action, en, ru)| {
            (
                app.text(en, ru).to_owned(),
                Target::Action(action),
                app.allowed(action),
            )
        })
        .collect();
    buttons(frame, app, parts[1], entries, palette);
    let mut player_placement = app.settings.player_placement;
    if area.width < 80 && matches!(player_placement, Placement::Left | Placement::Right) {
        player_placement = Placement::Bottom;
    }
    let vertical = matches!(player_placement, Placement::Left | Placement::Right);
    let player_first = matches!(player_placement, Placement::Top | Placement::Left);
    let size = if vertical { 30 } else { 7 };
    let content = Layout::default()
        .direction(if vertical {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints(if player_first {
            vec![Constraint::Length(size), Constraint::Min(5)]
        } else {
            vec![Constraint::Min(5), Constraint::Length(size)]
        })
        .split(parts[2]);
    let (player_area, library_area) = if player_first {
        (content[0], content[1])
    } else {
        (content[1], content[0])
    };
    player(frame, app, player_area, palette);
    let mut playlist_placement = app.settings.playlist_placement;
    if library_area.width < 70 && matches!(playlist_placement, Placement::Left | Placement::Right) {
        playlist_placement = Placement::Top;
    }
    let horizontal = matches!(playlist_placement, Placement::Left | Placement::Right);
    let list_first = matches!(playlist_placement, Placement::Left | Placement::Top);
    let size = if horizontal {
        (library_area.width / 4).clamp(20, 34)
    } else {
        7
    };
    let panels = Layout::default()
        .direction(if horizontal {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints(if list_first {
            vec![Constraint::Length(size), Constraint::Min(4)]
        } else {
            vec![Constraint::Min(4), Constraint::Length(size)]
        })
        .split(library_area);
    let (list, tracks) = if list_first {
        (panels[0], panels[1])
    } else {
        (panels[1], panels[0])
    };
    playlists(frame, app, list, palette);
    tracks_table(frame, app, tracks, palette);
    let notice = if app.busy() {
        format!(
            "{}: {}",
            app.text("Reading files", "Чтение файлов"),
            app.progress()
        )
    } else {
        app.notice.clone()
    };
    frame.render_widget(
        Paragraph::new(notice)
            .style(Style::default().fg(if app.notice_error {
                palette.error
            } else {
                palette.muted
            }))
            .wrap(Wrap { trim: false }),
        Rect::new(parts[3].x, parts[3].y, parts[3].width, 2),
    );
    frame.render_widget(
        Paragraph::new(app.text(
            "↑↓ navigate · Tab panel · Enter play · Space pause · F1 help · Q quit",
            "↑↓ выбор · Tab панель · Enter играть · Space пауза · F1 помощь · Q выход",
        ))
        .style(Style::default().fg(palette.muted))
        .wrap(Wrap { trim: false }),
        Rect::new(
            parts[3].x,
            parts[3].y + 2,
            parts[3].width,
            footer_height - 2,
        ),
    );
    if let Some(mut dialog) = app.dialog.take() {
        app.hits.clear();
        render_dialog(frame, app, &mut dialog, palette);
        app.dialog = Some(dialog);
    }
}

fn playlists(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.playlist_area = area;
    let outer = block(
        app.text("Playlists", "Плейлисты").into(),
        palette,
        app.focus == Focus::Playlists,
    );
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let items: Vec<(i64, String, usize)> = app
        .visible_playlists()
        .iter()
        .map(|playlist| {
            (
                playlist.id,
                playlist.display_name(app.settings.language).to_owned(),
                playlist.entries.len(),
            )
        })
        .collect();
    let selected = items
        .iter()
        .position(|(id, _, _)| Some(*id) == app.selected_playlist)
        .unwrap_or(0);
    app.playlist_offset = visible_offset(
        app.playlist_offset,
        selected,
        inner.height as usize,
        items.len(),
    );
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(app.text("Create a playlist", "Создайте плейлист"))
                .style(palette.text().fg(palette.muted))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }
    for (row, (id, name, count)) in items
        .iter()
        .skip(app.playlist_offset)
        .take(inner.height as usize)
        .enumerate()
    {
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let selected = Some(*id) == app.selected_playlist;
        frame.render_widget(
            Paragraph::new(format!(
                "{} {name} · {count}",
                if selected { ">" } else { " " }
            ))
            .style(if selected {
                palette.text().bg(palette.selection).fg(palette.accent)
            } else {
                palette.text()
            }),
            rect,
        );
        app.hits.push(Hit {
            area: rect,
            target: Target::Playlist(*id),
            enabled: true,
        });
    }
}

fn tracks_table(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    app.tracks_area = area;
    let name = app
        .playlist()
        .map(|playlist| playlist.display_name(app.settings.language))
        .unwrap_or(app.text("Tracks", "Композиции"));
    let title = format!(
        "{name} · {}{}",
        app.sort.name(app.settings.language),
        if app.query.is_empty() {
            String::new()
        } else {
            format!(" · {}", app.query)
        }
    );
    let outer = block(title, palette, app.focus == Focus::Tracks);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 2 {
        return;
    }
    let capacity = inner.height.saturating_sub(1) as usize;
    let entries = app.rows();
    let selected = entries
        .iter()
        .position(|entry| Some(entry.id) == app.selected_entry)
        .unwrap_or(0);
    let offset = visible_offset(app.track_offset, selected, capacity, entries.len());
    let ids: Vec<i64> = entries
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|entry| entry.id)
        .collect();
    let rows: Vec<_> = entries
        .iter()
        .skip(offset)
        .take(capacity)
        .map(|entry| {
            let selected = Some(entry.id) == app.selected_entry;
            let playing = app
                .current
                .as_ref()
                .is_some_and(|track| track.id == entry.track.id);
            let mark = if app.marked.contains(&entry.id) {
                "[x]"
            } else {
                "[ ]"
            };
            Row::new(vec![
                mark.to_owned(),
                format!("{}{}", if playing { ">" } else { "" }, entry.position + 1),
                clean(&entry.track.title),
                clean(&entry.track.artist),
                duration_text(entry.track.duration_ms),
            ])
            .style(if selected {
                palette.text().bg(palette.selection).fg(palette.accent)
            } else {
                palette.text()
            })
        })
        .collect();
    let empty = entries.is_empty();
    app.track_offset = offset;
    let columns = if inner.width < 55 {
        vec![
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(0),
            Constraint::Length(5),
        ]
    } else {
        vec![
            Constraint::Length(3),
            Constraint::Length(4),
            Constraint::Percentage(52),
            Constraint::Min(8),
            Constraint::Length(6),
        ]
    };
    let header = Row::new(vec![
        "".to_owned(),
        "#".to_owned(),
        app.text("Title", "Название").to_owned(),
        app.text("Artist", "Исполнитель").to_owned(),
        app.text("Time", "Время").to_owned(),
    ])
    .style(palette.text().fg(palette.muted));
    frame.render_widget(
        Table::new(rows, columns).header(header).column_spacing(1),
        inner,
    );
    if empty {
        frame.render_widget(
            Paragraph::new(app.text(
                "Add files, a folder, or copy tracks from the sorting desk.",
                "Добавьте файлы, папку или скопируйте композиции из сортировочного стола.",
            ))
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y + 2,
                inner.width,
                inner.height.saturating_sub(2),
            ),
        );
    }
    for (row, id) in ids.into_iter().enumerate() {
        let rect = Rect::new(inner.x, inner.y + 1 + row as u16, inner.width, 1);
        app.hits.push(Hit {
            area: rect,
            target: Target::Track(id),
            enabled: true,
        });
        app.hits.push(Hit {
            area: Rect::new(rect.x, rect.y, 3.min(rect.width), 1),
            target: Target::Mark(id),
            enabled: true,
        });
    }
}

fn player(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    let outer = block(app.text("Player", "Проигрыватель").into(), palette, false);
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    if inner.height < 3 {
        return;
    }
    let title = app
        .current
        .as_ref()
        .map(|track| format!("{} — {}", track.title, track.artist))
        .unwrap_or_else(|| {
            app.text("No track playing", "Ничего не воспроизводится")
                .into()
        });
    frame.render_widget(
        Paragraph::new(clean(&title)).style(palette.text().fg(palette.accent)),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let commands = [
        (Action::Previous, "<<", "<<"),
        (
            Action::TogglePlay,
            if app.paused() { "Play" } else { "Pause" },
            if app.paused() {
                "Играть"
            } else {
                "Пауза"
            },
        ),
        (Action::Stop, "Stop", "Стоп"),
        (Action::Next, ">>", ">>"),
        (Action::VolumeDown, "Vol -", "Громк -"),
        (Action::VolumeUp, "Vol +", "Громк +"),
    ];
    let entries = commands
        .into_iter()
        .map(|(action, en, ru)| {
            (
                app.text(en, ru).to_owned(),
                Target::Action(action),
                app.allowed(action),
            )
        })
        .collect();
    let used = buttons(
        frame,
        app,
        Rect::new(
            inner.x,
            inner.y + 1,
            inner.width,
            inner.height.saturating_sub(2),
        ),
        entries,
        palette,
    );
    let y = (inner.y + 1 + used).min(inner.bottom().saturating_sub(1));
    let duration = app.current.as_ref().map_or(0, |track| track.duration_ms);
    let position = app.position_ms();
    let ratio = if duration > 0 {
        (position as f64 / duration as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let rect = Rect::new(inner.x, y, inner.width, 1);
    frame.render_widget(
        Gauge::default()
            .ratio(ratio)
            .gauge_style(Style::default().fg(palette.accent).bg(palette.selection))
            .label(format!(
                "{} / {} · {}%",
                duration_text(position),
                duration_text(duration),
                (app.settings.volume * 100.0).round()
            )),
        rect,
    );
    app.hits.push(Hit {
        area: rect,
        target: Target::Seek(rect),
        enabled: duration > 0,
    });
}

fn modal(frame: &mut Frame, area: Rect, title: String, height: u16, palette: Palette) -> Rect {
    let width = area.width.saturating_sub(4).min(88);
    let height = height.min(area.height.saturating_sub(4));
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let outer = block(title, palette, true);
    let inner = outer.inner(rect);
    frame.render_widget(outer, rect);
    inner
}

fn render_dialog(frame: &mut Frame, app: &mut App, dialog: &mut Dialog, palette: Palette) {
    let mut area = frame.area();
    area.height = area.height.saturating_sub(3);
    match dialog {
        Dialog::Text(dialog) => text_dialog(frame, app, dialog, area, palette),
        Dialog::Browser(browser) => browser_dialog(frame, app, browser, area, palette),
        Dialog::RemoveEntry { title, .. } => {
            let inner = modal(
                frame,
                area,
                app.text("Remove track?", "Убрать композицию?").into(),
                10,
                palette,
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "{}\n\n{}",
                    title,
                    app.text(
                        "Remove from this playlist? The music file and saved tags are kept.",
                        "Убрать из этого плейлиста? Музыкальный файл и сохраненные теги останутся."
                    )
                ))
                .style(palette.text())
                .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(2),
                ),
            );
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (app.text("Remove", "Убрать").into(), Target::Submit, true),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::Transfer { selected, .. } => {
            let inner = modal(
                frame,
                area,
                app.text("Copy tracks to playlist", "Копировать в плейлист")
                    .into(),
                24,
                palette,
            );
            let entries: Vec<String> = app
                .transfer_destinations()
                .iter()
                .map(|playlist| {
                    format!(
                        "{} · {}",
                        playlist.display_name(app.settings.language),
                        playlist.mode.name(app.settings.language)
                    )
                })
                .collect();
            let capacity = inner.height.saturating_sub(3) as usize;
            let offset = visible_offset(0, *selected, capacity, entries.len());
            if entries.is_empty() {
                frame.render_widget(
                    Paragraph::new(app.text(
                        "Create a playlist or unlock Order editing.",
                        "Создайте плейлист или включите редактирование Порядка.",
                    ))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                    inner,
                );
            }
            for (row, label) in entries.iter().enumerate().skip(offset).take(capacity) {
                let rect = Rect::new(inner.x, inner.y + (row - offset) as u16, inner.width, 1);
                frame.render_widget(
                    Paragraph::new(label.clone()).style(if row == *selected {
                        palette.text().bg(palette.selection)
                    } else {
                        palette.text()
                    }),
                    rect,
                );
                app.hits.push(Hit {
                    area: rect,
                    target: Target::TransferRow(row),
                    enabled: true,
                });
            }
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
                vec![
                    (
                        app.text("Copy", "Копировать").into(),
                        Target::Submit,
                        !entries.is_empty(),
                    ),
                    (
                        app.text("Cancel", "Отмена").into(),
                        Target::CloseDialog,
                        true,
                    ),
                ],
                palette,
            );
        }
        Dialog::Settings { selected } => {
            let inner = modal(
                frame,
                area,
                app.text("Settings · current mode", "Настройки · текущий режим")
                    .into(),
                40,
                palette,
            );
            let theme = &app.settings.themes[app.settings.mode.index()];
            let mut rows = vec![
                format!(
                    "{}: {}",
                    app.text("Language", "Язык"),
                    if app.settings.language == Language::English {
                        "English"
                    } else {
                        "Русский"
                    }
                ),
                format!(
                    "{}: {}",
                    app.text("Theme", "Тема"),
                    app.text(
                        if theme.light { "Light" } else { "Dark" },
                        if theme.light {
                            "Светлая"
                        } else {
                            "Темная"
                        }
                    )
                ),
                format!(
                    "{}: #{:02X}{:02X}{:02X}",
                    app.text("Accent", "Акцент"),
                    theme.accent[0],
                    theme.accent[1],
                    theme.accent[2]
                ),
                format!(
                    "{}: {}",
                    app.text("Playlists panel", "Панель плейлистов"),
                    app.settings.playlist_placement.name(app.settings.language)
                ),
                format!(
                    "{}: {}",
                    app.text("Player panel", "Панель проигрывателя"),
                    app.settings.player_placement.name(app.settings.language)
                ),
                app.text("Reset shortcuts", "Сбросить горячие клавиши")
                    .into(),
            ];
            rows.extend(app.settings.bindings.iter().map(|binding| {
                format!(
                    "{} · {}",
                    binding.key.label(),
                    binding.action.name(app.settings.language)
                )
            }));
            let capacity = inner.height.saturating_sub(3) as usize;
            let offset = visible_offset(0, *selected, capacity, rows.len());
            for (row, label) in rows.iter().enumerate().skip(offset).take(capacity) {
                let rect = Rect::new(inner.x, inner.y + (row - offset) as u16, inner.width, 1);
                frame.render_widget(
                    Paragraph::new(label.clone()).style(if row == *selected {
                        palette.text().bg(palette.selection)
                    } else {
                        palette.text()
                    }),
                    rect,
                );
                app.hits.push(Hit {
                    area: rect,
                    target: Target::Setting(row),
                    enabled: true,
                });
            }
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::Help { offset } => {
            let inner = modal(
                frame,
                area,
                app.text("Guide and shortcuts", "Гайд и горячие клавиши")
                    .into(),
                44,
                palette,
            );
            let mut lines=vec![app.text("Mouse: click to select; double-click a track to play; scroll lists.","Мышь: клик — выбор; двойной клик по композиции — играть; колесо — прокрутка.").to_owned(),app.text("Use track checkboxes to select several tracks for copying.","Чекбоксы композиций отмечают несколько файлов для копирования.").into(),app.text("Keyboard: arrows select, Tab changes panel, Enter plays or confirms, Esc closes.","Клавиатура: стрелки — выбор, Tab — панель, Enter — играть или подтвердить, Esc — закрыть.").into(),String::new(),app.text("Order playlists are protected. Enable Edit to rename, remove, reorder or add directly.","Плейлисты Порядка защищены. Включите Правку для переименования, удаления, перемещения и прямого добавления.").into(),app.text("The sorting desk is always editable. Its checkbox routes every addition there.","Сортировочный стол всегда доступен для правки. Его чекбокс направляет туда все добавляемые файлы.").into(),app.text("Chaos and the desk support undo/redo in this session. Concurrent edits in the same scope may reset history. Only one app process can open this library for writing.","Хаос и стол поддерживают отмену/повтор в этой сессии. Одновременная правка тех же списков может очистить историю. Библиотеку для записи открывает только один процесс приложения.").into(),app.text("Sorting and searching change the view, never stored positions or source tags.","Сортировка и поиск меняют вид, а не сохраненные позиции или теги исходных файлов.").into(),app.text("Add new scans the folders of the current tracks; duplicates are skipped.","Добавить новые проверяет папки текущих композиций; повторные файлы пропускаются.").into(),format!("{}: {}",app.text("Database","База"),app.store.path.display()),String::new()];
            lines.extend(app.settings.bindings.iter().map(|binding| {
                format!(
                    "{} — {}",
                    binding.key.label(),
                    binding.action.name(app.settings.language)
                )
            }));
            *offset = (*offset).min(lines.len().saturating_sub(1));
            frame.render_widget(
                Paragraph::new(lines.join("\n"))
                    .scroll((*offset as u16, 0))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::Metadata { track, offset } => {
            let inner = modal(
                frame,
                area,
                app.text(
                    "Saved track information",
                    "Сохраненные сведения о композиции",
                )
                .into(),
                40,
                palette,
            );
            let mut lines = vec![
                format!("{}: {}", app.text("Title", "Название"), track.title),
                format!("{}: {}", app.text("Artist", "Исполнитель"), track.artist),
                format!("{}: {}", app.text("Album", "Альбом"), track.album),
                format!(
                    "{}: {}",
                    app.text("Duration", "Длительность"),
                    duration_text(track.duration_ms)
                ),
                format!("{}: {}", app.text("File", "Файл"), track.path.display()),
                String::new(),
                app.text(
                    "Original imported tags (read-only):",
                    "Исходные импортированные теги (только чтение):",
                )
                .into(),
            ];
            if let Ok(tags) =
                serde_json::from_str::<std::collections::BTreeMap<String, Vec<String>>>(&track.tags)
            {
                lines.extend(
                    tags.into_iter()
                        .map(|(key, values)| format!("{key}: {}", values.join("; "))),
                );
            }
            *offset = (*offset).min(lines.len().saturating_sub(1));
            frame.render_widget(
                Paragraph::new(lines.join("\n"))
                    .scroll((*offset as u16, 0))
                    .style(palette.text())
                    .wrap(Wrap { trim: false }),
                Rect::new(
                    inner.x,
                    inner.y,
                    inner.width,
                    inner.height.saturating_sub(3),
                ),
            );
            dialog_footer(frame, app, inner, palette);
        }
        Dialog::CaptureBinding { index } => {
            let name = app
                .settings
                .bindings
                .get(*index)
                .map(|binding| binding.action.name(app.settings.language))
                .unwrap_or("");
            let inner = modal(
                frame,
                area,
                app.text("Set shortcut", "Назначить сочетание").into(),
                10,
                palette,
            );
            frame.render_widget(Paragraph::new(format!("{name}\n\n{}",app.text("Press a new shortcut. Esc cancels. Existing shortcuts and navigation keys are protected.","Нажмите новое сочетание. Esc — отмена. Занятые сочетания и клавиши навигации защищены."))).style(palette.text()).wrap(Wrap{trim:false}),inner);
            buttons(
                frame,
                app,
                Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
                vec![(
                    app.text("Cancel", "Отмена").into(),
                    Target::CloseDialog,
                    true,
                )],
                palette,
            );
        }
    }
}

fn text_dialog(
    frame: &mut Frame,
    app: &mut App,
    dialog: &TextDialog,
    area: Rect,
    palette: Palette,
) {
    let title = match &dialog.purpose {
        TextPurpose::Create => app.text("Create playlist", "Создать плейлист"),
        TextPurpose::RenamePlaylist(_) => app.text("Rename playlist", "Переименовать плейлист"),
        TextPurpose::RenameTrack(_, _) => app.text(
            "Local track name · source tags untouched",
            "Имя в базе · исходные теги сохраняются",
        ),
        TextPurpose::Search => app.text(
            "Search · title, artist, album, path",
            "Поиск · название, исполнитель, альбом, путь",
        ),
        TextPurpose::DeletePlaylist(_, _) => app.text(
            "Remove playlist · confirmation",
            "Удалить плейлист · подтверждение",
        ),
        TextPurpose::Accent => app.text("Accent color · RRGGBB", "Цвет акцента · RRGGBB"),
    };
    let inner = modal(frame, area, title.into(), 27, palette);
    let hint = if let TextPurpose::DeletePlaylist(_, name) = &dialog.purpose {
        format!(
            "{}: {name}",
            app.text("Enter exact name", "Введите точное имя")
        )
    } else {
        app.text(
            "Enter: save · Esc: cancel",
            "Enter: сохранить · Esc: отмена",
        )
        .into()
    };
    let hint_height = (Line::from(hint.clone())
        .width()
        .div_ceil(inner.width.max(1) as usize) as u16
        + 1)
    .min(10);
    frame.render_widget(
        Paragraph::new(hint)
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, hint_height),
    );
    let field = Rect::new(inner.x, inner.y + hint_height, inner.width, 3);
    frame.render_widget(
        Paragraph::new(format!("{}|", dialog.text))
            .scroll((
                0,
                dialog
                    .text
                    .chars()
                    .count()
                    .saturating_sub(inner.width.saturating_sub(4) as usize) as u16,
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(
                palette
                    .text()
                    .fg(palette.accent)
                    .bg(if dialog.selected_all {
                        palette.selection
                    } else {
                        palette.panel
                    }),
            ),
        field,
    );
    let keyboard = if dialog.keyboard == Language::Russian {
        vec![
            "йцукенгшщзхъ",
            "фывапролджэ",
            "ячсмитьбю",
            "1234567890",
            "ё _-().",
        ]
    } else {
        vec!["qwertyuiop", "asdfghjkl", "zxcvbnm", "1234567890", " _-()."]
    };
    for (row, letters) in keyboard.iter().enumerate() {
        let y = field.bottom() + 1 + row as u16;
        if y >= inner.bottom().saturating_sub(4) {
            break;
        }
        for (column, c) in letters.chars().enumerate() {
            let c = if dialog.upper {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c
            };
            let x = inner.x + column as u16 * 3;
            if x + 3 <= inner.right() {
                button(
                    frame,
                    app,
                    Rect::new(x, y, 3, 1),
                    &c.to_string(),
                    Target::Text(c),
                    true,
                    palette,
                );
            }
        }
    }
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(4), inner.width, 2),
        vec![
            ("RU / EN".into(), Target::KeyboardLanguage, true),
            ("Shift".into(), Target::KeyboardCase, true),
            ("Backspace".into(), Target::Backspace, true),
        ],
        palette,
    );
    let can_submit = match &dialog.purpose {
        TextPurpose::DeletePlaylist(_, name) => &dialog.text == name,
        _ => true,
    };
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            (
                app.text("Confirm", "Подтвердить").into(),
                Target::Submit,
                can_submit,
            ),
            (
                app.text("Cancel", "Отмена").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}

fn browser_dialog(
    frame: &mut Frame,
    app: &mut App,
    browser: &mut Browser,
    area: Rect,
    palette: Palette,
) {
    let inner = modal(
        frame,
        area,
        app.text(
            if browser.folder {
                "Add folder"
            } else {
                "Add files"
            },
            if browser.folder {
                "Добавить папку"
            } else {
                "Добавить файлы"
            },
        )
        .into(),
        42,
        palette,
    );
    frame.render_widget(
        Paragraph::new(browser.directory.display().to_string())
            .style(palette.text().fg(palette.muted))
            .wrap(Wrap { trim: false }),
        Rect::new(inner.x, inner.y, inner.width, 2),
    );
    let used = buttons(
        frame,
        app,
        Rect::new(inner.x, inner.y + 2, inner.width, 3),
        vec![
            (
                app.text("Parent", "Выше").into(),
                Target::BrowserParent,
                browser.directory.parent().is_some(),
            ),
            (
                app.text("Open", "Открыть").into(),
                Target::BrowserOpen,
                !browser.entries.is_empty(),
            ),
            (
                app.text("Mark all", "Отметить все").into(),
                Target::BrowserMarkAll,
                !browser.folder,
            ),
        ],
        palette,
    );
    let list = Rect::new(
        inner.x,
        inner.y + 2 + used,
        inner.width,
        inner.height.saturating_sub(4 + used),
    );
    browser.offset = visible_offset(
        browser.offset,
        browser.selected,
        list.height as usize,
        browser.entries.len(),
    );
    for (index, entry) in browser
        .entries
        .iter()
        .enumerate()
        .skip(browser.offset)
        .take(list.height as usize)
    {
        let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
        let marker = if entry.directory {
            "DIR"
        } else if browser.marked.contains(&entry.path) {
            "[x]"
        } else {
            "[ ]"
        };
        let rect = Rect::new(
            list.x,
            list.y + (index - browser.offset) as u16,
            list.width,
            1,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{marker} {}{}",
                clean(&name),
                if entry.directory { "/" } else { "" }
            ))
            .style(if browser.selected == index {
                palette.text().bg(palette.selection)
            } else {
                palette.text()
            }),
            rect,
        );
        app.hits.push(Hit {
            area: rect,
            target: Target::BrowserRow(index),
            enabled: true,
        });
    }
    let add = if browser.folder {
        app.text("Add folder · Ctrl+Enter", "Добавить папку · Ctrl+Enter")
            .into()
    } else {
        format!(
            "{} ({}) · Ctrl+Enter",
            app.text("Add selected", "Добавить выбранные"),
            browser.marked.len()
        )
    };
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            (
                add,
                Target::BrowserAdd,
                browser.folder
                    || !browser.marked.is_empty()
                    || browser
                        .entries
                        .get(browser.selected)
                        .is_some_and(|entry| !entry.directory),
            ),
            (
                app.text("Cancel", "Отмена").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}

fn dialog_footer(frame: &mut Frame, app: &mut App, inner: Rect, palette: Palette) {
    buttons(
        frame,
        app,
        Rect::new(inner.x, inner.bottom().saturating_sub(2), inner.width, 2),
        vec![
            ("↑".into(), Target::DialogScroll(-5), true),
            ("↓".into(), Target::DialogScroll(5), true),
            (
                app.text("Close", "Закрыть").into(),
                Target::CloseDialog,
                true,
            ),
        ],
        palette,
    );
}
fn visible_offset(previous: usize, selected: usize, capacity: usize, length: usize) -> usize {
    let capacity = capacity.max(1);
    let mut offset = previous.min(length.saturating_sub(capacity));
    if selected < offset {
        offset = selected;
    } else if selected >= offset + capacity {
        offset = selected + 1 - capacity;
    }
    offset
}
fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
