use super::{App, Dialog, Target};
use crate::{
    input::{Key, KeyPress},
    tuner::{SLIDERS, TunerSettings},
};
use anyhow::Result;
use ratatui::layout::Position;

impl App {
    pub(super) fn open_tuner(&mut self) {
        self.cancel_workspace_drag();
        self.view.dialog = Some(Dialog::PlaybackTuner { selected: 0 });
    }

    pub(super) fn sync_tuner(&mut self) -> Result<()> {
        self.sync_playback_direction()?;
        let Some(audio) = &self.playback.audio else {
            return Ok(());
        };
        audio.tuner(&self.settings.tuner);
        let reverse = self
            .playback
            .seek
            .requested()
            .map_or(audio.reversed(), |request| request.reverse);
        if reverse != self.settings.tuner.reverse && !self.preparing_playback() {
            let mut position = self.position_ms();
            if self.settings.tuner.reverse
                && position <= 1
                && let Some(track) = &self.playback.current
            {
                position = track.duration_ms.saturating_sub(1);
            }
            self.seek_to(position);
            self.message(
                if self.settings.tuner.reverse {
                    self.text(
                        "Preparing reverse playback…",
                        "Подготовка обратного проигрывания…",
                    )
                } else {
                    self.text(
                        "Preparing forward playback…",
                        "Подготовка прямого проигрывания…",
                    )
                }
                .into(),
            );
        }
        Ok(())
    }

    fn select_tuner(&mut self, index: usize) {
        if let Some(Dialog::PlaybackTuner { selected }) = &mut self.view.dialog {
            *selected = index.min(SLIDERS - 1);
        }
    }

    fn set_tuner_value(&mut self, index: usize, value: u16) -> Result<()> {
        let value = value.clamp(TunerSettings::minimum(index), TunerSettings::maximum(index));
        if self.settings.tuner.value(index) != Some(value) && index < SLIDERS {
            self.settings.tuner.set_value(index, value);
            self.save_settings()?;
        }
        Ok(())
    }

    fn adjust_tuner(&mut self, index: usize, step: i16) -> Result<()> {
        if let Some(value) = self.settings.tuner.value(index) {
            self.select_tuner(index);
            self.set_tuner_value(
                index,
                (i32::from(value) + i32::from(step)).clamp(0, i32::from(u16::MAX)) as u16,
            )?;
        }
        Ok(())
    }

    pub(super) fn tuner_target(&mut self, target: Target) -> Result<()> {
        match target {
            Target::TunerReverse => {
                self.settings.tuner.reverse = !self.settings.tuner.reverse;
                self.save_settings()?;
            }
            Target::TunerReset => {
                self.settings.tuner = Default::default();
                self.save_settings()?;
            }
            Target::TunerSelect(index) => self.select_tuner(index),
            Target::TunerSlider(index, area) if index < SLIDERS && area.width > 0 => {
                self.select_tuner(index);
                if self.view.workspace.gesture.is_none() {
                    self.view.workspace.gesture =
                        Some(crate::workspace::Gesture::TunerSlider(index, area));
                }
                let point = self.view.pointer.x.clamp(area.x, area.right() - 1) - area.x;
                let minimum = TunerSettings::minimum(index);
                let maximum = TunerSettings::maximum(index);
                let value = f32::from(minimum)
                    + f32::from(point) * f32::from(maximum - minimum)
                        / f32::from(area.width.saturating_sub(1).max(1));
                self.set_tuner_value(index, value.round() as u16)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn tuner_key(&mut self, key: KeyPress) -> Result<()> {
        let Some(Dialog::PlaybackTuner { selected }) = self.view.dialog else {
            return Ok(());
        };
        if key.ctrl || key.alt {
            return Ok(());
        }
        match key.key {
            Key::Char(' ') => self.tuner_target(Target::TunerReverse)?,
            Key::Char('r') => self.tuner_target(Target::TunerReset)?,
            Key::Up | Key::Down | Key::Tab => {
                let backwards = key.key == Key::Up || (key.key == Key::Tab && key.shift);
                self.select_tuner(if backwards {
                    (selected + SLIDERS - 1) % SLIDERS
                } else {
                    (selected + 1) % SLIDERS
                });
            }
            Key::Left => self.adjust_tuner(selected, -1)?,
            Key::Right => self.adjust_tuner(selected, 1)?,
            Key::PageDown => self.adjust_tuner(selected, -10)?,
            Key::PageUp => self.adjust_tuner(selected, 10)?,
            Key::Home => self.set_tuner_value(selected, TunerSettings::normal(selected))?,
            _ => {}
        }
        self.view.pointer = Position::new(u16::MAX, u16::MAX);
        Ok(())
    }

    pub(super) fn scroll_tuner(&mut self, position: Position, delta: i16) -> Result<()> {
        if let Some(Target::TunerSlider(index, _) | Target::TunerSelect(index)) =
            self.target_at(position).cloned()
        {
            self.adjust_tuner(index, -delta.signum())?;
        }
        Ok(())
    }
}
