use crate::input::Action;
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
        self.base_height(width) + if self.waveform { 4 } else { 0 }
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

    fn control_cells(&self, width: u16) -> Vec<(Rect, Action, &'static str)> {
        let mut x: u16 = 0;
        let mut y = 0;
        self.controls
            .iter()
            .map(|control| {
                let size = control.width.min(width);
                if x > 0 && x.saturating_add(size) > width {
                    x = 0;
                    y += 1;
                }
                let rect = Rect::new(x, y, size, 1);
                x = x.saturating_add(size + 1);
                (rect, control.action, control.label)
            })
            .collect()
    }

    fn control_rows(&self, width: u16) -> u16 {
        self.control_cells(width)
            .last()
            .map_or(1, |(area, ..)| area.bottom())
    }

    fn loop_rows(&self, width: u16) -> u16 {
        u16::from(width < 70 && self.checked_loop)
    }

    fn base_height(&self, width: u16) -> u16 {
        let height = if self.stack_volume(width) {
            self.control_rows(width).saturating_add(3)
        } else {
            2
        };
        height + self.loop_rows(width)
    }

    pub fn arrange(&self, area: Rect) -> PlayerGeometry {
        let loop_rows = self.loop_rows(area.width);
        let row = Rect::new(area.x, area.y + 1 + loop_rows, area.width, 1);
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
                    self.control_rows(area.width)
                } else {
                    0
                },
            volume_width,
            1,
        );
        let controls: Vec<_> = self
            .control_cells(area.width)
            .into_iter()
            .map(|(cell, action, label)| {
                (
                    Rect::new(row.x + cell.x, row.y + cell.y, cell.width, cell.height),
                    action,
                    label,
                )
            })
            .collect();
        let x = controls.last().map_or(row.x, |(rect, ..)| rect.right() + 1);
        let progress = if !stacked {
            Rect::new(x + 1, row.y, volume.x.saturating_sub(x + 2), 1)
        } else if area.height >= self.base_height(area.width) {
            Rect::new(area.x, volume.bottom(), area.width, 1)
        } else {
            Rect::default()
        };
        let wave_y = area.y + self.base_height(area.width);
        let waveform = (self.waveform && wave_y + 4 <= area.bottom())
            .then(|| Rect::new(area.x, wave_y, area.width, 4));
        PlayerGeometry {
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
