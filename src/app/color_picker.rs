use super::{App, Dialog, Target};
use crate::{
    color_picker::{ColorPicker, Hsv},
    input::{Key, KeyPress},
    preferences::color_hex,
    workspace::Gesture,
};
use ratatui::layout::{Position, Rect};

impl App {
    fn update_color(&mut self, change: impl FnOnce(&mut ColorPicker)) -> bool {
        let Some(Dialog::Text(dialog)) = &mut self.view.dialog else {
            return false;
        };
        let Some(color) = &mut dialog.color_picker else {
            return false;
        };
        change(color);
        dialog.text = color_hex(color.hsv.rgb());
        dialog.selected_all = true;
        self.view.notice_error = false;
        true
    }

    pub fn drag_color_wheel(&mut self, area: Rect, x: f64, y: f64, release: bool) {
        if area.is_empty() || !x.is_finite() || !y.is_finite() {
            return;
        }
        if self.update_color(|color| {
            color.pick_wheel(x, y);
            color.selected = Some(0);
        }) && self.view.workspace.gesture.is_none()
        {
            self.view.workspace.gesture = Some(Gesture::ColorWheel(area));
        }
        if release {
            self.view.workspace.gesture = None;
        }
    }

    pub fn drag_color_slider(&mut self, index: usize, area: Rect, ratio: f64, release: bool) {
        if index >= ColorPicker::CHANNELS || area.is_empty() || !ratio.is_finite() {
            return;
        }
        if self.update_color(|color| {
            color.set_channel(
                index,
                (ratio.clamp(0.0, 1.0) * ColorPicker::maximum(index)).round(),
            );
            color.selected = Some(index);
        }) && self.view.workspace.gesture.is_none()
        {
            self.view.workspace.gesture = Some(Gesture::ColorSlider(index, area));
        }
        if release {
            self.view.workspace.gesture = None;
        }
    }

    pub(super) fn color_target(&mut self, target: Target) {
        match target {
            Target::ColorWheel(area) if !area.is_empty() => {
                let x = 2.0 * (f64::from(self.view.pointer.x) - f64::from(area.x))
                    / f64::from(area.width.saturating_sub(1).max(1))
                    - 1.0;
                let y = 2.0 * (f64::from(self.view.pointer.y) - f64::from(area.y))
                    / f64::from(area.height.saturating_sub(1).max(1))
                    - 1.0;
                if self.view.workspace.gesture.is_some() || x.hypot(y) <= 1.05 {
                    self.drag_color_wheel(area, x, y, false);
                }
            }
            Target::ColorSlider(index, area) if !area.is_empty() => {
                let x = self.view.pointer.x.clamp(area.x, area.right() - 1) - area.x;
                self.drag_color_slider(
                    index,
                    area,
                    f64::from(x) / f64::from(area.width.saturating_sub(1).max(1)),
                    false,
                );
            }
            Target::ColorSelect(index) if index < ColorPicker::CHANNELS => {
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog
                    && let Some(color) = &mut dialog.color_picker
                {
                    color.selected = Some(index);
                }
            }
            Target::ColorHex => {
                if let Some(Dialog::Text(dialog)) = &mut self.view.dialog
                    && let Some(color) = &mut dialog.color_picker
                {
                    color.selected = None;
                    dialog.selected_all = true;
                }
            }
            Target::ColorReset => {
                self.update_color(|color| {
                    color.hsv = Hsv::from_rgb(color.original);
                    color.selected = None;
                });
            }
            _ => {}
        }
    }

    fn adjust_color_channel(&mut self, index: usize, direction: f64) {
        self.update_color(|color| {
            color.set_channel(index, color.channel(index).round() + direction);
            color.selected = Some(index);
        });
    }

    pub(super) fn color_picker_key(&mut self, key: KeyPress) -> bool {
        let Some(Dialog::Text(dialog)) = &mut self.view.dialog else {
            return false;
        };
        let Some(color) = &mut dialog.color_picker else {
            return false;
        };
        if key.ctrl || key.alt {
            if key.ctrl && key.key == Key::Char('a') {
                color.selected = None;
            }
            return false;
        }
        match key.key {
            Key::Tab | Key::Up | Key::Down => {
                let direction = if key.key == Key::Up || (key.key == Key::Tab && key.shift) {
                    -1
                } else {
                    1
                };
                let next = (color.selected.unwrap_or(ColorPicker::CHANNELS) as i32 + direction)
                    .rem_euclid(ColorPicker::CHANNELS as i32 + 1)
                    as usize;
                color.selected = (next < ColorPicker::CHANNELS).then_some(next);
                dialog.selected_all = true;
            }
            Key::Left | Key::Right | Key::PageUp | Key::PageDown if color.selected.is_some() => {
                let index = color.selected.unwrap();
                let step = if key.shift || matches!(key.key, Key::PageUp | Key::PageDown) {
                    10.0
                } else {
                    1.0
                };
                let direction = if matches!(key.key, Key::Left | Key::PageDown) {
                    -step
                } else {
                    step
                };
                self.adjust_color_channel(index, direction);
            }
            Key::Home | Key::End if color.selected.is_some() => {
                let index = color.selected.unwrap();
                let value = if key.key == Key::Home {
                    0.0
                } else {
                    ColorPicker::maximum(index)
                };
                self.update_color(|color| color.set_channel(index, value));
            }
            _ => return false,
        }
        true
    }

    pub(super) fn color_picker_scroll(&mut self, position: Position, direction: i16) -> bool {
        if !matches!(&self.view.dialog, Some(Dialog::Text(dialog)) if dialog.color_picker.is_some())
        {
            return false;
        }
        let index = match self.target_at(position) {
            Some(Target::ColorSlider(index, _) | Target::ColorSelect(index)) => *index,
            Some(Target::ColorWheel(_)) => 2,
            _ => return false,
        };
        self.adjust_color_channel(index, -f64::from(direction));
        true
    }
}
