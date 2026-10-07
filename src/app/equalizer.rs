use super::{App, Dialog, Target};
use crate::{
    equalizer::{MAX_GAIN, MIN_GAIN, PRESETS, SLIDERS},
    input::{Key, KeyPress},
};
use anyhow::Result;

impl App {
    pub(super) fn open_equalizer(&mut self) {
        self.cancel_workspace_drag();
        self.view.dialog = Some(Dialog::Equalizer {
            selected: 0,
            presets: false,
            preset_selected: self.settings.equalizer.preset().unwrap_or(0),
        });
    }

    pub(super) fn equalizer_target(&mut self, target: Target) -> Result<()> {
        match target {
            Target::EqualizerToggle => {
                self.settings.equalizer.enabled = !self.settings.equalizer.enabled;
                self.save_settings()?;
            }
            Target::EqualizerPresets => {
                let current = self.settings.equalizer.preset().unwrap_or(0);
                if let Some(Dialog::Equalizer {
                    presets,
                    preset_selected,
                    ..
                }) = &mut self.view.dialog
                {
                    *presets = !*presets;
                    *preset_selected = current;
                }
            }
            Target::EqualizerPreset(index) => {
                if let Some(preset) = PRESETS.get(index) {
                    self.settings.equalizer.preamp = preset.preamp;
                    self.settings.equalizer.bands = preset.bands;
                    if let Some(Dialog::Equalizer {
                        presets,
                        preset_selected,
                        ..
                    }) = &mut self.view.dialog
                    {
                        *presets = false;
                        *preset_selected = index;
                    }
                    self.save_settings()?;
                }
            }
            Target::EqualizerReset => {
                self.settings.equalizer.preamp = 0;
                self.settings.equalizer.bands = [0; 10];
                self.save_settings()?;
            }
            Target::EqualizerSelect(index) => self.select_equalizer(index),
            Target::EqualizerGain {
                index,
                area,
                vertical,
            } if index < SLIDERS && area.width > 0 && area.height > 0 => {
                self.select_equalizer(index);
                if self.view.workspace.gesture.is_none() {
                    self.view.workspace.gesture = Some(crate::workspace::Gesture::EqualizerGain {
                        index,
                        area,
                        vertical,
                    });
                }
                let (point, length) = if vertical {
                    (
                        area.bottom()
                            .saturating_sub(1)
                            .saturating_sub(self.view.pointer.y.clamp(area.y, area.bottom() - 1)),
                        area.height,
                    )
                } else {
                    (
                        self.view.pointer.x.clamp(area.x, area.right() - 1) - area.x,
                        area.width,
                    )
                };
                let gain = f32::from(MIN_GAIN)
                    + f32::from(point) * f32::from(MAX_GAIN - MIN_GAIN)
                        / f32::from(length.saturating_sub(1).max(1));
                self.set_equalizer_gain(index, gain.round() as i8)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn select_equalizer(&mut self, index: usize) {
        if let Some(Dialog::Equalizer { selected, .. }) = &mut self.view.dialog {
            *selected = index.min(SLIDERS - 1);
        }
    }

    fn set_equalizer_gain(&mut self, index: usize, gain: i8) -> Result<()> {
        if self.settings.equalizer.gain(index) != Some(gain.clamp(MIN_GAIN, MAX_GAIN)) {
            self.settings.equalizer.set_gain(index, gain);
            self.save_settings()?;
        }
        Ok(())
    }

    pub(super) fn adjust_equalizer_gain(&mut self, index: usize, step: i8) -> Result<()> {
        if let Some(gain) = self.settings.equalizer.gain(index) {
            self.select_equalizer(index);
            self.set_equalizer_gain(index, gain.saturating_add(step))?;
        }
        Ok(())
    }

    pub(super) fn equalizer_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::Equalizer {
            selected,
            presets,
            preset_selected,
        }) = self.view.dialog.as_ref()
        else {
            return Ok(());
        };
        let (selected, presets, preset_selected) = (*selected, *presets, *preset_selected);
        if key.ctrl || key.alt {
            return Ok(());
        }
        if let Key::Char(character @ '1'..='8') = key.key {
            return self
                .equalizer_target(Target::EqualizerPreset(character as usize - '1' as usize));
        }
        match key.key {
            Key::Char(' ') => self.equalizer_target(Target::EqualizerToggle)?,
            Key::Char('r') => self.equalizer_target(Target::EqualizerReset)?,
            Key::Enter if presets => {
                self.equalizer_target(Target::EqualizerPreset(preset_selected))?
            }
            Key::Enter => self.equalizer_target(Target::EqualizerPresets)?,
            Key::Up | Key::Down if presets => {
                if let Some(Dialog::Equalizer {
                    preset_selected, ..
                }) = &mut self.view.dialog
                {
                    *preset_selected = super::bounded(
                        *preset_selected,
                        if key.key == Key::Up { -1 } else { 1 },
                        PRESETS.len(),
                    );
                }
            }
            Key::Left | Key::Right | Key::Tab => {
                let backwards = key.key == Key::Left || (key.key == Key::Tab && key.shift);
                self.select_equalizer(if backwards {
                    (selected + SLIDERS - 1) % SLIDERS
                } else {
                    (selected + 1) % SLIDERS
                });
            }
            Key::Up => self.adjust_equalizer_gain(selected, 1)?,
            Key::Down => self.adjust_equalizer_gain(selected, -1)?,
            Key::PageUp => self.adjust_equalizer_gain(selected, 3)?,
            Key::PageDown => self.adjust_equalizer_gain(selected, -3)?,
            Key::Home => self.set_equalizer_gain(selected, 0)?,
            _ => {}
        }
        self.view.pointer = ratatui::layout::Position::new(u16::MAX, u16::MAX);
        Ok(())
    }

    pub(super) fn scroll_equalizer(
        &mut self,
        position: ratatui::layout::Position,
        delta: i16,
    ) -> Result<()> {
        match self.target_at(position).cloned() {
            Some(Target::EqualizerGain { index, .. } | Target::EqualizerSelect(index)) => {
                self.adjust_equalizer_gain(index, -delta.signum() as i8)?
            }
            _ => {
                if let Some(Dialog::Equalizer {
                    presets: true,
                    preset_selected,
                    ..
                }) = &mut self.view.dialog
                {
                    *preset_selected =
                        super::bounded(*preset_selected, i64::from(delta.signum()), PRESETS.len());
                }
            }
        }
        Ok(())
    }
}
