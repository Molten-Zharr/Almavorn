use crate::{input::Action, preferences::PlayerButtonSize};
use ratatui::layout::Rect;

pub(crate) struct PlayerControl {
    pub action: Action,
    pub label: &'static str,
    pub width: u16,
}

#[derive(Default)]
pub(crate) struct PlayerMetrics {
    pub controls: Vec<PlayerControl>,
    pub checked_loop: bool,
    pub waveform: bool,
}

pub(crate) struct PlayerGeometry {
    pub header: Rect,
    pub artwork: Option<Rect>,
    pub controls: Vec<(Rect, Action, &'static str)>,
    pub volume: Rect,
    pub progress: Rect,
    pub waveform: Option<Rect>,
    pub loop_rows: u16,
}

impl PlayerMetrics {
    pub fn preferred_height(&self, width: u16) -> u16 {
        self.content_height(width.saturating_sub(2)) + 2
    }

    pub fn content_height(&self, width: u16) -> u16 {
        self.content_height_sized(width, PlayerButtonSize::Compact)
    }

    pub fn content_height_sized(&self, width: u16, size: PlayerButtonSize) -> u16 {
        self.base_height(width, size) + if self.waveform { 4 } else { 0 }
    }

    fn artwork_width(&self, width: u16, maximum: u16) -> Option<u16> {
        let width = width.saturating_sub(30).min(maximum) / 2 * 2;
        (width >= 4).then_some(width)
    }

    pub fn content_height_with_artwork(&self, width: u16, maximum: u16) -> u16 {
        self.content_height_with_artwork_sized(width, maximum, PlayerButtonSize::Compact)
    }

    pub fn content_height_with_artwork_sized(&self, width: u16, maximum: u16, size: PlayerButtonSize) -> u16 {
        let Some(cover_width) = self.artwork_width(width, maximum) else {
            return self.content_height_sized(width, size);
        };
        self.base_height(width - cover_width - 1, size)
            .max(cover_width / 2)
            + if self.waveform { 4 } else { 0 }
    }

    pub fn arrange_with_artwork(&self, area: Rect, maximum: u16) -> PlayerGeometry {
        self.arrange_with_artwork_sized(area, maximum, PlayerButtonSize::Compact)
    }

    pub fn arrange_with_artwork_sized(&self, area: Rect, maximum: u16, size: PlayerButtonSize) -> PlayerGeometry {
        let Some(limit) = self.artwork_width(area.width, maximum) else {
            return self.arrange_sized(area, size);
        };
        let Some(width) = (4..=limit).rev().step_by(2).find(|&width| {
            area.height >= self.content_height_with_artwork_sized(area.width, width, size)
        }) else {
            return self.arrange_sized(area, size);
        };
        let text_width = area.width - width - 1;
        let text_height = self.base_height(text_width, size);
        let header_height = text_height.max(width / 2);
        let header = Rect::new(
            area.x + width + 1,
            area.y + (header_height - text_height) / 2,
            text_width,
            text_height,
        );
        let mut geometry = self.arrange_sized(header, size);
        geometry.artwork = Some(Rect::new(
            area.x,
            area.y + (header_height - width / 2) / 2,
            width,
            width / 2,
        ));
        geometry.waveform = self.waveform.then(|| {
            Rect::new(area.x, area.y + header_height, area.width, 4)
        });
        geometry
    }

    fn controls_width(&self) -> u16 {
        self.controls
            .iter()
            .map(|control| control.width.saturating_add(1))
            .fold(0, u16::saturating_add)
    }

    fn volume_width(&self, width: u16) -> u16 {
        let available = width.saturating_sub(self.controls_width().saturating_add(15));
        (width / 3).clamp(24, 80).min(available.max(24)).min(width)
    }

    fn stack_volume(&self, width: u16) -> bool {
        width < 70
            || self
                .controls_width()
                .saturating_add(self.volume_width(width))
                .saturating_add(15)
                > width
    }

    fn control_cells(&self, width: u16, button_size: PlayerButtonSize) -> Vec<(Rect, Action, &'static str)> {
        let mut x: u16 = 0;
        let mut y = 0;
        self.controls
            .iter()
            .map(|control| {
                let size = control.width.min(width);
                if x > 0 && x.saturating_add(size) > width {
                    x = 0;
                    y += button_size.height();
                }
                let rect = Rect::new(x, y, size, button_size.height());
                x = x.saturating_add(size + 1);
                (rect, control.action, control.label)
            })
            .collect()
    }

    fn control_rows(&self, width: u16, size: PlayerButtonSize) -> u16 {
        self.control_cells(width, size)
            .last()
            .map_or(1, |(area, ..)| area.bottom())
    }

    fn loop_rows(&self, width: u16) -> u16 {
        u16::from(width < 70 && self.checked_loop)
    }

    fn base_height(&self, width: u16, size: PlayerButtonSize) -> u16 {
        let height = if self.stack_volume(width) {
            self.control_rows(width, size).saturating_add(3)
        } else {
            size.height() + 1
        };
        height + self.loop_rows(width) + u16::from(size != PlayerButtonSize::Compact)
    }

    pub fn arrange(&self, area: Rect) -> PlayerGeometry {
        self.arrange_sized(area, PlayerButtonSize::Compact)
    }

    pub fn arrange_sized(&self, area: Rect, size: PlayerButtonSize) -> PlayerGeometry {
        let loop_rows = self.loop_rows(area.width);
        let row = Rect::new(area.x, area.y + 1 + loop_rows + u16::from(size != PlayerButtonSize::Compact), area.width, size.height());
        let stacked = self.stack_volume(area.width);
        let volume_width = if stacked {
            area.width
        } else {
            self.volume_width(area.width)
        };
        let volume = Rect::new(
            row.right().saturating_sub(volume_width),
            row.y
                + if stacked {
                    self.control_rows(area.width, size)
                } else {
                    size.height() / 2
                },
            volume_width,
            1,
        );
        let controls: Vec<_> = self
            .control_cells(area.width, size)
            .into_iter()
            .map(|(cell, action, label)| {
                (
                    Rect::new(row.x + cell.x, row.y + cell.y, cell.width, cell.height).intersection(area),
                    action,
                    label,
                )
            })
            .collect();
        let x = controls.last().map_or(row.x, |(rect, ..)| rect.right() + 1);
        let progress = if !stacked {
            Rect::new(x + 1, volume.y, volume.x.saturating_sub(x + 2), 1)
        } else if area.height >= self.base_height(area.width, size) {
            Rect::new(area.x, volume.bottom(), area.width, 1)
        } else {
            Rect::default()
        };
        let wave_y = area.y + self.base_height(area.width, size);
        let waveform = (self.waveform && wave_y + 4 <= area.bottom())
            .then(|| Rect::new(area.x, wave_y, area.width, 4));
        PlayerGeometry {
            header: area,
            artwork: None,
            controls,
            volume,
            progress,
            waveform,
            loop_rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(widths: &[u16]) -> PlayerMetrics {
        PlayerMetrics {
            controls: widths
                .iter()
                .map(|width| PlayerControl {
                    action: Action::TogglePlay,
                    label: "play",
                    width: *width,
                })
                .collect(),
            checked_loop: true,
            waveform: true,
        }
    }

    #[test]
    fn narrow_player_stacks_volume_and_checked_loop_while_wide_player_keeps_one_control_row() {
        let metrics = metrics(&[6, 6, 6]);
        let narrow = metrics.arrange(Rect::new(10, 20, 69, 9));
        assert_eq!(metrics.content_height(69), 9);
        assert_eq!(narrow.loop_rows, 1);
        assert_eq!(narrow.volume, Rect::new(10, 23, 69, 1));
        assert_eq!(narrow.progress, Rect::new(10, 24, 69, 1));
        assert_eq!(narrow.waveform, Some(Rect::new(10, 25, 69, 4)));
        let wide = metrics.arrange(Rect::new(10, 20, 70, 6));
        assert_eq!(metrics.content_height(70), 6);
        assert_eq!(wide.loop_rows, 0);
        assert_eq!(wide.volume, Rect::new(56, 21, 24, 1));
        assert_eq!(wide.progress, Rect::new(32, 21, 23, 1));
        assert_eq!(wide.waveform, Some(Rect::new(10, 22, 70, 4)));
    }

    #[test]
    fn long_captions_wrap_without_overlapping_volume_progress_or_waveform() {
        let metrics = metrics(&[20, 20, 20]);
        let area = Rect::new(7, 11, 32, metrics.content_height(32));
        let geometry = metrics.arrange(area);
        assert_eq!(
            geometry
                .controls
                .iter()
                .map(|(rect, ..)| rect.y)
                .collect::<Vec<_>>(),
            [13, 14, 15]
        );
        assert_eq!(geometry.volume.y, 16);
        assert_eq!(geometry.progress.y, 17);
        assert_eq!(geometry.waveform, Some(Rect::new(7, 18, 32, 4)));
        let mut areas: Vec<_> = geometry.controls.iter().map(|(rect, ..)| *rect).collect();
        areas.extend([
            geometry.volume,
            geometry.progress,
            geometry.waveform.unwrap(),
        ]);
        for (index, first) in areas.iter().enumerate() {
            assert_eq!(first.intersection(area), *first);
            for second in &areas[index + 1..] {
                assert!(first.intersection(*second).is_empty());
            }
        }
    }

    #[test]
    fn waveform_requires_four_full_rows_and_empty_controls_still_leave_volume_reachable() {
        let metrics = metrics(&[]);
        let height = metrics.content_height(32);
        assert!(
            metrics
                .arrange(Rect::new(0, 0, 32, height - 1))
                .waveform
                .is_none()
        );
        let full = metrics.arrange(Rect::new(0, 0, 32, height));
        assert!(full.controls.is_empty());
        assert_eq!(full.volume, Rect::new(0, 3, 32, 1));
        assert_eq!(full.progress, Rect::new(0, 4, 32, 1));
        assert!(full.waveform.is_some());
        assert_eq!(metrics.preferred_height(34), height + 2);
    }
}
