use crate::{
    app::{App, Dialog, SettingsPage},
    ui::{
        dialogs::{help, mods, services},
        theme::Palette,
    },
};
use ratatui::{Frame, layout::Rect};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect, palette: Palette) {
    if let Some(text) = app.settings_guide() {
        let mut offset = app.view.settings.offset;
        help::copyable_text(frame, app, &mut offset, area, text, palette);
        app.view.settings.offset = offset;
        return;
    }
    // Dialogs retain selection and operation state; their content now lives in
    // Settings. Forms and confirmations draw over the same settings page.
    let base = app.view.dialog.as_ref().map(Dialog::settings_base);
    if app.settings_page() == SettingsPage::Mods {
        let mut view = match base {
            Some(Dialog::Mods(view)) => view.clone(),
            _ => Default::default(),
        };
        view.remove = None;
        view.install = None;
        mods::render_content(frame, app, &mut view, area, palette);
        if let Some(Dialog::Mods(current)) = &mut app.view.dialog
            && current.remove.is_none()
            && current.install.is_none()
        {
            *current = view;
        }
    } else {
        let mut view = match base {
            Some(Dialog::Services(view)) => view.clone(),
            _ => Default::default(),
        };
        view.remove = None;
        services::render_content(frame, app, &mut view, area, palette);
        if let Some(Dialog::Services(current)) = &mut app.view.dialog
            && current.remove.is_none()
        {
            *current = view;
        }
    }
}
