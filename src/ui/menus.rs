use super::theme::Palette;
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
    text::Span,
};

pub(super) fn hint(app: &App) -> Option<String> {
    let Some(Dialog::Commands { menu, selected, .. }) = &app.view.dialog else {
        return None;
    };
    // Read the previous frame's menu regions before the main view rebuilds them.
    // Disabled rows still explain their action when the pointer is over them.
    let hovered = match app.hovered_target() { Some(Target::CommandRow(index)) => Some(*index), _ => None };
    let items = app.command_items(*menu);
    let item = hovered
        .and_then(|index| items.get(index))
        .or_else(|| items.get(*selected))?;
    let description = match item.target {
        Target::ToggleArtworkSearch => app.text(
            "Find artwork with MusicBrainz and Cover Art Archive, sending the artist and track title to MusicBrainz.",
            "Искать обложки через MusicBrainz и Cover Art Archive, передавая исполнителя и название композиции в MusicBrainz.",
        ),
        Target::RevealFile(_) => app.text(
            "Show this audio file in your file manager. Online tracks have no local file.",
            "Показать аудиофайл в проводнике. У онлайн-композиций нет локального файла.",
        ),
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
    let area = Rect { height: frame.area().height.saturating_sub(1), ..frame.area() };
    let width = items.iter().map(|item| Span::raw(&item.label).width()
        + displayed_shortcut(app, &item.target).map_or(0, |key| Span::raw(key).width() + 3) + 6)
        .max().unwrap_or(24).min(usize::from(area.width.saturating_sub(2))) as u16;
    let hints = app.settings.preferences.appearance.show_button_shortcuts;
    let height = super::dropdown::height(app, &items, hints).min(area.height.saturating_sub(2));
    let outer = Rect::new(anchor.x.min(area.right().saturating_sub(width)),
        anchor.y.saturating_add(1).min(area.bottom().saturating_sub(height)), width, height);
    app.view.overlay_area = outer;
    app.view.hits.push(Hit { area: frame.area(), target: Target::CloseDialog, enabled: true });
    *selected = App::command_selection(&items, *selected, 0);
    super::dropdown::render(frame, app, outer, menu.title(app.settings.preferences.language).into(),
        &items, *selected, hints.then_some("↑↓ · Enter · Esc"), palette, |index, _| Target::CommandRow(index));
}
