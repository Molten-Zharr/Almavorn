use super::{App, Dialog, Target, TextDialog};
use crate::{
    color_picker::{ColorPicker, Hsv},
    input::{Key, KeyPress},
    preferences::color_hex,
    workspace::Gesture,
};
use ratatui::{
    layout::{Position, Rect},
    text::Span,
};

impl TextDialog {
    pub fn color_hex_selection(&self) -> std::ops::Range<usize> {
        let color = self.color_picker.as_ref().expect("color picker");
        let length = self.text.chars().count();
        let cursor = color.hex_cursor.min(length);
        let anchor = color.hex_anchor.min(length);
        cursor.min(anchor)..cursor.max(anchor)
    }

    pub fn color_hex_scroll(&self, width: u16) -> usize {
        let color = self.color_picker.as_ref().expect("color picker");
        self.text
            .chars()
            .take(color.hex_cursor)
            .map(|c| Span::raw(c.to_string()).width())
            .sum::<usize>()
            .saturating_sub(usize::from(width.saturating_sub(1)))
    }

    fn select_color_hex(&mut self) {
        if let Some(color) = &mut self.color_picker {
            color.hex_cursor = self.text.chars().count();
            color.hex_anchor = 0;
        }
    }

    pub(super) fn insert_color_hex(&mut self, text: &str) -> bool {
        if self.color_picker.is_none() {
            return false;
        }
        let mut characters: Vec<_> = self.text.chars().collect();
        let selection = self.color_hex_selection();
        let remaining = 512usize.saturating_sub(characters.len() - selection.len());
        let inserted: Vec<_> = text
            .chars()
            .filter(|c| !c.is_control())
            .take(remaining)
            .collect();
        let cursor = selection.start + inserted.len();
        characters.splice(selection, inserted);
        self.text = characters.into_iter().collect();
        let color = self.color_picker.as_mut().expect("color picker");
        color.hex_cursor = cursor;
        color.hex_anchor = cursor;
        self.selected_all = false;
        self.sync_color_text();
        true
    }

    fn color_hex_at(&self, area: Rect, point: Position) -> usize {
        let column =
            self.color_hex_scroll(area.width) + usize::from(point.x.saturating_sub(area.x));
        let mut x = 0;
        for (index, c) in self.text.chars().enumerate() {
            let width = Span::raw(c.to_string()).width();
            if column < x + width {
                return index;
            }
            x += width;
        }
        self.text.chars().count()
    }
}

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
        dialog.select_color_hex();
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
            Target::ColorReset => {
                self.update_color(|color| {
                    color.hsv = Hsv::from_rgb(color.original);
                    color.selected = None;
                });
            }
            _ => {}
        }
    }

    pub(super) fn click_color_hex(&mut self, area: Rect, double: bool) {
        if let Some(Dialog::Text(dialog)) = &mut self.view.dialog
            && dialog.color_picker.is_some()
        {
            let cursor = dialog.color_hex_at(area, self.view.pointer);
            if let Some(color) = &mut dialog.color_picker {
                color.selected = None;
                color.hex_cursor = cursor;
                color.hex_anchor = cursor;
                if double {
                    dialog.select_color_hex();
                } else {
                    self.view.workspace.gesture = Some(Gesture::ColorHex(area));
                }
            }
        }
    }

    pub(super) fn drag_color_hex(&mut self, area: Rect, point: Position) {
        if let Some(Dialog::Text(dialog)) = &mut self.view.dialog
            && dialog.color_picker.is_some()
        {
            let cursor = dialog.color_hex_at(area, point);
            if let Some(color) = &mut dialog.color_picker {
                color.hex_cursor = cursor;
            }
        }
    }

    pub fn copy_color_hex(&mut self, cut: bool) -> bool {
        let Some(Dialog::Text(dialog)) = &mut self.view.dialog else {
            return false;
        };
        let Some(color) = &dialog.color_picker else {
            return false;
        };
        if color.selected.is_none() {
            let selection = dialog.color_hex_selection();
            if !selection.is_empty() {
                self.view.clipboard_text = Some(
                    dialog
                        .text
                        .chars()
                        .skip(selection.start)
                        .take(selection.len())
                        .collect(),
                );
                if cut {
                    dialog.insert_color_hex("");
                }
            }
        }
        true
    }

    fn color_hex_key(&mut self, key: KeyPress) -> bool {
        let Some(Dialog::Text(dialog)) = &mut self.view.dialog else {
            return false;
        };
        let Some(color) = &dialog.color_picker else {
            return false;
        };
        if key.alt {
            return false;
        }
        if key.ctrl {
            match key.normalized().key {
                Key::Char('a') => {
                    dialog.color_picker.as_mut().unwrap().selected = None;
                    dialog.select_color_hex();
                    return true;
                }
                Key::Char('c') | Key::Insert => return self.copy_color_hex(false),
                Key::Char('x') => return self.copy_color_hex(true),
                _ => {}
            }
        }
        if color.selected.is_some() {
            return false;
        }
        let length = dialog.text.chars().count();
        let selection = dialog.color_hex_selection();
        let color = dialog.color_picker.as_mut().unwrap();
        let cursor = color.hex_cursor.min(length);
        let next = match key.key {
            Key::Home => 0,
            Key::End => length,
            Key::Left if key.ctrl => 0,
            Key::Right if key.ctrl => length,
            Key::Left if !key.shift && !selection.is_empty() => selection.start,
            Key::Right if !key.shift && !selection.is_empty() => selection.end,
            Key::Left => cursor.saturating_sub(1),
            Key::Right => (cursor + 1).min(length),
            Key::Backspace | Key::Delete => {
                if key.shift && key.key == Key::Delete {
                    return self.copy_color_hex(true);
                }
                if selection.is_empty() {
                    color.hex_anchor = cursor;
                    color.hex_cursor = match (key.key, key.ctrl) {
                        (Key::Backspace, true) => 0,
                        (Key::Backspace, false) => cursor.saturating_sub(1),
                        (_, true) => length,
                        (_, false) => (cursor + 1).min(length),
                    };
                }
                dialog.insert_color_hex("");
                return true;
            }
            _ => return false,
        };
        color.hex_cursor = next;
        if !key.shift {
            color.hex_anchor = next;
        }
        true
    }

    fn adjust_color_channel(&mut self, index: usize, direction: f64) {
        self.update_color(|color| {
            color.set_channel(index, color.channel(index).round() + direction);
            color.selected = Some(index);
        });
    }

    pub(super) fn color_picker_key(&mut self, key: KeyPress) -> bool {
        if self.color_hex_key(key) {
            return true;
        }
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
                dialog.select_color_hex();
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
