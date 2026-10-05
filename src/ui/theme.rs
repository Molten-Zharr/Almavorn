use crate::{
    app::App,
    preferences::{BorderWeight, Corners},
};
use ratatui::style::{Color, Style};

#[derive(Clone, Copy)]
pub(super) struct Palette {
    pub(super) background: Color,
    pub(super) panel: Color,
    pub(super) text: Color,
    pub(super) muted: Color,
    pub(super) accent: Color,
    pub(super) selection: Color,
    pub(super) error: Color,
    pub(super) button_text: Color,
    pub(super) sidebar_title: Color,
    pub(super) sidebar_selection: Color,
    pub(super) inactive_file_border: Color,
    pub(super) inactive_panel_border: Color,
    pub(super) folder_icon: Color,
    pub(super) folder_path: Color,
    pub(super) selection_text: Color,
    pub(super) confirm_background: Color,
    pub(super) help_heading: Color,
    pub(super) separator: Color,
    pub(super) borders: BorderWeight,
    pub(super) corners: Corners,
}
impl Palette {
    pub(super) fn new(app: &App) -> Self {
        let palette = app.settings.current_palette();
        let color = |key| {
            let [r, g, b] = palette.color(key);
            Color::Rgb(r, g, b)
        };
        Self {
            background: color("background"),
            panel: color("background"),
            text: color("text"),
            muted: color("folder_path"),
            accent: color("active"),
            selection: color("selected_file_background"),
            error: color("error"),
            button_text: color("button_text"),
            sidebar_title: color("sidebar_title"),
            sidebar_selection: color("selection_accent"),
            inactive_file_border: color("inactive_file_border"),
            inactive_panel_border: color("inactive_panel_border"),
            folder_icon: color("folder_icon"),
            folder_path: color("folder_path"),
            selection_text: color("selection_accent"),
            confirm_background: color("confirm_background"),
            help_heading: color("help_heading"),
            separator: color("separator"),
            borders: app.settings.appearance.borders,
            corners: app.settings.appearance.corners,
        }
    }
    pub(super) fn text(self) -> Style {
        Style::default().fg(self.text).bg(self.panel)
    }
}
