use super::{App, Dialog, Target};
use crate::{
    input::{Key, KeyPress},
    tuner::{SLIDERS, TunerSettings},
};
use anyhow::Result;
use ratatui::layout::{Position, Rect};
use std::path::Path;

impl App {
    pub(super) fn tuner_for_path(&self, path: &Path) -> &TunerSettings {
        self.settings
            .track_tuners
            .get(path)
            .unwrap_or(&self.settings.tuner)
    }

    /// FX shown in the dialog and applied to the currently playing track.
    pub fn tuner_settings(&self) -> &TunerSettings {
        self.playback
            .current
            .as_ref()
            .map_or(&self.settings.tuner, |track| {
                self.tuner_for_path(&track.path)
            })
    }

    pub fn tuner_pinned(&self) -> bool {
        self.playback
            .current
            .as_ref()
            .is_some_and(|track| self.settings.track_tuners.contains_key(&track.path))
    }

    fn editable_tuner(&mut self) -> &mut TunerSettings {
        if let Some(track) = &self.playback.current
            && let Some(tuner) = self.settings.track_tuners.get_mut(&track.path)
        {
            tuner
        } else {
            &mut self.settings.tuner
        }
    }

    fn toggle_tuner_pin(&mut self) -> Result<()> {
        let Some(track) = &self.playback.current else {
            return Ok(());
        };
        let path = track.path.clone();
        if self.settings.track_tuners.remove(&path).is_none() {
            self.settings
                .track_tuners
                .insert(path, self.settings.tuner.clone());
        }
        self.cancel_workspace_drag();
        self.sync_tuner()?;
        self.save_settings()
    }

    pub(super) fn open_tuner(&mut self) {
        self.cancel_workspace_drag();
        self.view.dialog = Some(Dialog::PlaybackTuner { selected: 0 });
    }

    pub(super) fn sync_tuner(&mut self) -> Result<()> {
        self.sync_playback_direction()?;
        let tuner = self.tuner_settings().clone();
        let Some(audio) = &self.playback.audio else {
            return Ok(());
        };
        audio.tuner(&tuner);
        let reverse = self
            .playback
            .seek
            .requested()
            .map_or(audio.reversed(), |request| request.reverse);
        if reverse != tuner.reverse && !self.preparing_playback() {
            let mut position = self.position_ms();
            if tuner.reverse
                && position <= 1
                && let Some(track) = &self.playback.current
            {
                position = track.duration_ms.saturating_sub(1);
            }
            self.seek_to(position);
            self.message(
                if tuner.reverse {
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
        if self.tuner_settings().value(index) != Some(value) && index < SLIDERS {
            self.editable_tuner().set_value(index, value);
            self.sync_tuner()?;
            self.save_settings()?;
        }
        Ok(())
    }

    fn adjust_tuner(&mut self, index: usize, step: i16) -> Result<()> {
        if let Some(value) = self.tuner_settings().value(index) {
            self.select_tuner(index);
            self.set_tuner_value(
                index,
                (i32::from(value) + i32::from(step)).clamp(0, i32::from(u16::MAX)) as u16,
            )?;
        }
        Ok(())
    }

    fn tuner_slider(&mut self, index: usize, area: Rect, ratio: f64) -> Result<()> {
        self.select_tuner(index);
        if self.view.workspace.gesture.is_none() {
            self.view.workspace.gesture = Some(crate::workspace::Gesture::TunerSlider(index, area));
        }
        let minimum = TunerSettings::minimum(index);
        let maximum = TunerSettings::maximum(index);
        let value = f64::from(minimum) + ratio.clamp(0.0, 1.0) * f64::from(maximum - minimum);
        self.set_tuner_value(index, value.round() as u16)
    }

    /// GUI drags keep subcell precision instead of rounding to terminal columns.
    pub fn drag_tuner(&mut self, index: usize, area: Rect, ratio: f64, release: bool) {
        if !matches!(self.view.dialog, Some(Dialog::PlaybackTuner { .. }))
            || index >= SLIDERS
            || area.width == 0
            || !ratio.is_finite()
        {
            return;
        }
        if let Err(error) = self.tuner_slider(index, area, ratio) {
            self.failure(error);
        }
        if release {
            self.view.workspace.gesture = None;
        }
    }

    pub(super) fn tuner_target(&mut self, target: Target) -> Result<()> {
        match target {
            Target::TunerReverse => {
                let tuner = self.editable_tuner();
                tuner.reverse = !tuner.reverse;
                self.sync_tuner()?;
                self.save_settings()?;
            }
            Target::TunerReset => {
                *self.editable_tuner() = Default::default();
                self.sync_tuner()?;
                self.save_settings()?;
            }
            Target::TunerPin => self.toggle_tuner_pin()?,
            Target::TunerSelect(index) => self.select_tuner(index),
            Target::TunerSlider(index, area) if index < SLIDERS && area.width > 0 => {
                let point = self.view.pointer.x.clamp(area.x, area.right() - 1) - area.x;
                let ratio = f64::from(point) / f64::from(area.width.saturating_sub(1).max(1));
                self.tuner_slider(index, area, ratio)?;
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
            Key::Char('p') => self.tuner_target(Target::TunerPin)?,
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
