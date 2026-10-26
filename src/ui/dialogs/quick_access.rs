use super::super::{theme::Palette, widgets::{buttons, dialog_footer, modal, visible_offset}};
use crate::{app::{App, Hit, Target}, toolbar::{QuickAction, QuickButton}};
use ratatui::{Frame, layout::Rect, widgets::Paragraph};

pub(super) fn render(frame: &mut Frame, app: &mut App, selected: &mut usize, area: Rect, palette: Palette) {
    let inner = modal(frame, app, area, app.text("Quick access", "Панель быстрого доступа").into(), 36, palette);
    let actions = vec![
        (app.text("Show all", "Показать все").into(), Target::QuickAccess(QuickAction::All(true)), true),
        (app.text("Hide all", "Убрать все").into(), Target::QuickAccess(QuickAction::All(false)), true),
        (app.text("Defaults", "По умолчанию").into(), Target::QuickAccess(QuickAction::Reset), true),
    ];
    let top = buttons(frame, app, Rect::new(inner.x, inner.y, inner.width, inner.height.min(4)), actions, palette);
    let active = format!("[{}] {}", if app.settings.preferences.appearance.quick_access.show_active { 'x' } else { ' ' }, app.text("Show layout editing while active", "Показывать активное редактирование блоков"));
    let toggle = buttons(frame, app, Rect::new(inner.x, inner.y + top, inner.width, inner.height.saturating_sub(top).min(3)), vec![(active, Target::QuickAccess(QuickAction::Active), true)], palette);
    let start = inner.y + top + toggle + 1;
    let capacity = usize::from(inner.bottom().saturating_sub(start + 3));
    *selected = (*selected).min(QuickButton::ALL.len() - 1);
    let offset = visible_offset(0, *selected, capacity / 2, QuickButton::ALL.len());
    let mut y = start;
    let mut category = "";
    for (index, button) in QuickButton::ALL.iter().enumerate().skip(offset) {
        let next = button.category(app.settings.preferences.language);
        if next != category {
            if y + 2 > inner.bottom().saturating_sub(3) { break; }
            frame.render_widget(Paragraph::new(next).style(palette.text().fg(palette.accent)), Rect::new(inner.x, y, inner.width, 1));
            y += 1;
            category = next;
        }
        if y >= inner.bottom().saturating_sub(3) { break; }
        let row = Rect::new(inner.x, y, inner.width, 1);
        let checked = app.settings.preferences.appearance.quick_access.pinned.contains(button);
        let style = if index == *selected { palette.text().bg(palette.selection).fg(palette.selection_text) } else { palette.text() };
        frame.render_widget(Paragraph::new(format!("[{}] {}", if checked { 'x' } else { ' ' }, button.name(app.settings.preferences.language))).style(style), row);
        app.view.hits.push(Hit { area: row, target: Target::QuickAccess(QuickAction::Toggle(index)), enabled: true });
        y += 1;
    }
    dialog_footer(frame, app, inner, palette);
}
