use super::{
    theme::Palette,
    widgets::{block, visible_offset},
};
use crate::{
    app::{App, CommandMenu, Dialog, Hit, Target},
    input::Action,
    model::Mode,
    visualizer::VisualizerKind,
    layout::buttons::displayed_shortcut,
};
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

pub(super) fn hint(app: &App) -> Option<String> {
    let Some(Dialog::Commands { menu, selected, .. }) = &app.view.dialog else {
        return None;
    };
    // Read the previous frame's menu regions before the main view rebuilds them.
    // Disabled rows still explain their action when the pointer is over them.
    let hovered = app.view.hits.iter().rev().find_map(|hit| match hit.target {
        Target::CommandRow(index) if app.hovered(hit.area) => Some(index),
        _ => None,
    });
    let items = app.command_items(*menu);
    let item = hovered
        .and_then(|index| items.get(index))
        .or_else(|| items.get(*selected))?;
    let description = match item.target {
        Target::GroupAction(action) => match action {
            crate::app::GroupAction::Collapse => app.text("Expand or collapse this group.", "Развернуть или свернуть эту группу."),
            crate::app::GroupAction::Mark => app.text("Check or uncheck every track in this group, including tracks hidden by the filter.", "Отметить или снять отметки со всей группы, включая скрытые фильтром композиции."),
            crate::app::GroupAction::Remove => app.text("Remove this group's tracks from the playlist after confirmation. Music files stay on disk.", "После подтверждения убрать композиции этой группы из плейлиста. Музыкальные файлы останутся на диске."),
            crate::app::GroupAction::EnableEdit => app.text("Allow changes to this playlist and return to the group menu.", "Разрешить изменения этого плейлиста и вернуться в меню группы."),
        },
        Target::Action(Action::Undo | Action::Redo) => {
            return Some(app.history_caption(matches!(item.target, Target::Action(Action::Undo))));
        }
        Target::Action(Action::ToggleDesk) => app.text(
            "Send newly added music to the sorting desk.",
            "Направлять добавляемую музыку на сортировочный стол.",
        ),
        Target::Action(action) => action.description(app.settings.preferences.language),
        Target::Mode(Mode::Order) => app.text(
            "Open saved playlists protected from accidental changes.",
            "Открыть сохраненные плейлисты с защитой от случайных изменений.",
        ),
        Target::Mode(Mode::Chaos) => app.text(
            "Open mixes you can freely combine and edit.",
            "Открыть подборки для свободного смешивания и редактирования.",
        ),
        Target::ToggleExpandedToolbar => app.text(
            "Open More: library tools, services, appearance and help.",
            "Открыть меню Больше: библиотека, сервисы, оформление и справка.",
        ),
        Target::TogglePlayerSpecialButtons => app.text(
            "Show or hide the EQ, FX and VIS buttons.",
            "Показать или скрыть кнопки EQ, FX и VIS.",
        ),
        Target::ToggleLyricsPanel => app.text(
            "Show lyrics for the playing song beside the playlist.",
            "Показывать текст играющей песни рядом с плейлистом.",
        ),
        Target::ManagePanels => app.text(
            "Choose which player panels and buttons are visible.",
            "Выбрать видимые панели и кнопки плеера.",
        ),
        Target::FinishEdit => app.text(
            "Finish editing and protect the playlist again.",
            "Завершить редактирование и снова защитить плейлист.",
        ),
        Target::CommandMenu(CommandMenu::Visualizer) => app.text(
            "Choose a background animation or turn it off.",
            "Выбрать анимацию фона или отключить ее.",
        ),
        Target::Visualizer(kind) => match kind {
            VisualizerKind::Gif => app.text("Show a GIF file in the background.", "Показать GIF-файл на фоне."),
            VisualizerKind::Off => app.text(
                "Turn off background animation.",
                "Отключить фоновую анимацию.",
            ),
            VisualizerKind::Spectrum => app.text(
                "Show the music spectrum in the background.",
                "Показать частотный спектр музыки на фоне.",
            ),
            VisualizerKind::Wave => app.text(
                "Show the audio waveform in the background.",
                "Показать звуковую волну на фоне.",
            ),
            VisualizerKind::Rings => app.text(
                "Show rings pulsing to the music.",
                "Показать пульсирующие кольца в такт музыке.",
            ),
            VisualizerKind::Particles => app.text(
                "Show sparks reacting to the music.",
                "Показать искры, реагирующие на музыку.",
            ),
            VisualizerKind::Bombardment => app.text(
                "Show falling bombs and explosions with the music.",
                "Показать падающие бомбы и взрывы под музыку.",
            ),
        },
        _ => return None,
    };
    Some(
        description
            .split_once(". ")
            .map_or(description, |(first, _)| first)
            .to_owned(),
    )
}

pub(super) fn render(
    frame: &mut Frame,
    app: &mut App,
    menu: CommandMenu,
    selected: &mut usize,
    anchor: Position,
    palette: Palette,
) {
    let items = app.command_items(menu);
    let area = Rect {
        height: frame.area().height.saturating_sub(1),
        ..frame.area()
    };
    let width = items
        .iter()
        .map(|item| {
            Span::raw(&item.label).width()
                + displayed_shortcut(app, &item.target).map_or(0, |key| Span::raw(key).width() + 3)
                + 4
        })
        .max()
        .unwrap_or(24)
        .min(usize::from(area.width.saturating_sub(2))) as u16;
    let row_height = if area.height >= 12 && menu != CommandMenu::More { 2 } else { 1 };
    let hint_height = u16::from(app.settings.preferences.appearance.show_button_shortcuts);
    let height = (items.len() as u16 * row_height + 2 + hint_height).min(area.height.saturating_sub(2));
    let outer = Rect::new(
        anchor.x.min(area.right().saturating_sub(width)),
        anchor
            .y
            .saturating_add(1)
            .min(area.bottom().saturating_sub(height)),
        width,
        height,
    );
    let block = block(
        menu.title(app.settings.preferences.language).into(),
        palette,
        false,
    )
    .border_style(palette.text().fg(palette.separator));
    let inner = block.inner(outer);
    app.view.overlay_area = outer;
    app.view.hits.push(Hit {
        area: frame.area(),
        target: Target::CloseDialog,
        enabled: true,
    });
    frame.render_widget(Clear, outer);
    frame.render_widget(block, outer);
    *selected = App::command_selection(&items, *selected, 0);
    let capacity = (inner.height.saturating_sub(hint_height) / row_height) as usize;
    let offset = visible_offset(0, *selected, capacity, items.len());
    for (index, item) in items.iter().enumerate().skip(offset).take(capacity) {
        let rect = Rect::new(inner.x, inner.y + (index - offset) as u16 * row_height, inner.width, row_height);
        let caption_y = rect.y + row_height / 2;
        if matches!(item.target, Target::MenuSection) {
            frame.render_widget(
                Paragraph::new(format!(" {}", item.label)).style(palette.text().fg(palette.accent)),
                rect,
            );
            app.view.hits.push(Hit { area: rect, target: Target::MenuSection, enabled: true });
            continue;
        }
        let style = if !item.enabled {
            palette.text().fg(palette.muted)
        } else if index == *selected {
            palette
                .text()
                .bg(palette.selection)
                .fg(palette.selection_text)
        } else {
            palette.text()
        };
        frame.render_widget(Paragraph::new("").style(style), rect);
        let shortcut = displayed_shortcut(app, &item.target);
        let key_width = shortcut
            .as_ref()
            .map_or(0, |key| Span::raw(key).width() as u16)
            .min(rect.width.saturating_sub(1));
        let label_width =
            rect.width
                .saturating_sub(if shortcut.is_some() { key_width + 2 } else { 0 });
        let label = if matches!(&item.target, Target::Action(Action::PlayMarked))
            && Span::raw(&item.label).width() + 1 > usize::from(label_width)
        {
            format!(
                "[{}] {}",
                if item.label.starts_with("[x]") {
                    'x'
                } else {
                    ' '
                },
                app.text("Checked tracks only", "Только отмеченные")
            )
        } else {
            item.label.clone()
        };
        frame.render_widget(
            Paragraph::new(format!(" {label}")).style(style),
            Rect::new(rect.x, caption_y, label_width, 1),
        );
        if let Some(key) = shortcut {
            frame.render_widget(
                Paragraph::new(key).style(style.fg(palette.muted)),
                Rect::new(
                    rect.right().saturating_sub(key_width + 1),
                    caption_y,
                    key_width,
                    1,
                ),
            );
        }
        app.view.hits.push(Hit {
            area: rect,
            target: Target::CommandRow(index),
            enabled: item.enabled,
        });
    }
    if hint_height > 0 { frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            app.text("↑↓ · Enter · Esc", "↑↓ · Enter · Esc"),
            palette.text().fg(palette.muted),
        )])),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    ); }
}
