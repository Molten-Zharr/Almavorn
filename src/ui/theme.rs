use crate::app::App;
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
}

impl Palette {
    pub(super) fn new(app: &App) -> Self {
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
    pub(super) fn text(self) -> Style {
        Style::default().fg(self.text).bg(self.panel)
    }
}
