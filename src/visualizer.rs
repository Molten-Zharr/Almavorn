//! Music-driven animation state, independent of terminal size and rendering.
use crate::{
    audio::{AnalysisFrame, SPECTRUM_BANDS, WAVE_SAMPLES},
    model::Language,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VisualizerKind {
    Spectrum,
    Wave,
    Rings,
    Particles,
    #[default]
    Bombardment,
    Off,
}
impl VisualizerKind {
    pub const ALL: [Self; 6] = [
        Self::Spectrum,
        Self::Wave,
        Self::Rings,
        Self::Particles,
        Self::Bombardment,
        Self::Off,
    ];
    pub fn name(self, language: Language) -> &'static str {
        match self {
            Self::Off => language.text("No animation", "Без анимации"),
            Self::Spectrum => language.text("Spectrum", "Спектр"),
            Self::Wave => language.text("Wave", "Аудиоволна"),
            Self::Rings => language.text("Pulse rings", "Пульсирующие кольца"),
            Self::Particles => language.text("Particles", "Частицы"),
            Self::Bombardment => language.text("Carpet bombing", "Ковровая бомбардировка"),
        }
    }
    pub fn next(self, direction: i64) -> Self {
        let index = Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .unwrap_or_default();
        Self::ALL[(index as i64 + direction.rem_euclid(Self::ALL.len() as i64))
            .rem_euclid(Self::ALL.len() as i64) as usize]
    }
}

#[derive(Clone, Debug)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub age: f32,
    pub power: f32,
}
#[derive(Clone, Debug)]
pub struct Pulse {
    pub age: f32,
    pub power: f32,
}
#[derive(Clone, Debug)]
pub struct Bomb {
    pub x: f32,
    pub y: f32,
    pub target_x: f32,
    pub target_y: f32,
    pub age: f32,
    pub power: f32,
}
#[derive(Clone, Debug)]
pub struct Impact {
    pub x: f32,
    pub y: f32,
    pub age: f32,
    pub power: f32,
}

#[derive(Clone, Debug)]
pub struct VisualizerState {
    pub available: bool,
    pub elapsed: f32,
    pub level: f32,
    pub bass: f32,
    pub treble: f32,
    pub flash: f32,
    pub bands: [f32; SPECTRUM_BANDS],
    pub wave: [f32; WAVE_SAMPLES],
    pub particles: Vec<Particle>,
    pub pulses: Vec<Pulse>,
    pub bombs: Vec<Bomb>,
    pub impacts: Vec<Impact>,
    sequence: u64,
    beats: u64,
    stale: f32,
    salvo: f32,
    emission: f32,
    flight: f32,
    random: u32,
}
impl Default for VisualizerState {
    fn default() -> Self {
        Self {
            available: false,
            elapsed: 0.0,
            level: 0.0,
            bass: 0.0,
            treble: 0.0,
            flash: 0.0,
            bands: [0.0; SPECTRUM_BANDS],
            wave: [0.0; WAVE_SAMPLES],
            particles: Vec::new(),
            pulses: Vec::new(),
            bombs: Vec::new(),
            impacts: Vec::new(),
            sequence: 0,
            beats: 0,
            stale: 0.0,
            salvo: 0.0,
            emission: 0.0,
            flight: 0.0,
            random: 0x71c34a59,
        }
    }
}
impl VisualizerState {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    fn random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random as f32 / u32::MAX as f32
    }
    pub fn plane(&self, index: usize) -> (f32, f32) {
        (
            (self.flight + 0.45 - index as f32 * 0.16).rem_euclid(1.5) - 0.25,
            0.16 + index as f32 * 0.075,
        )
    }
    fn burst(&mut self, power: f32, count: usize) {
        for _ in 0..count.min(192usize.saturating_sub(self.particles.len())) {
            let angle = self.random() * TAU;
            let speed = (0.09 + self.random() * 0.16) * (0.5 + power);
            self.particles.push(Particle {
                x: 0.5,
                y: 0.5,
                vx: angle.cos() * speed,
                vy: angle.sin() * speed,
                age: 0.0,
                power,
            });
        }
    }
    fn drop_salvo(&mut self, power: f32) {
        let index = (0..3).find(|&i| {
            let (x, _) = self.plane(i);
            (0.0..0.85).contains(&x)
        });
        let Some(index) = index else {
            return;
        };
        let (x, y) = self.plane(index);
        for i in 0..7.min(64usize.saturating_sub(self.bombs.len())) {
            let offset = i as f32 * 0.028;
            let target_y = 0.77 + self.random() * 0.15;
            self.bombs.push(Bomb {
                x: x + offset,
                y,
                target_x: (x + 0.13 + offset).min(0.98),
                target_y,
                age: -(i as f32) * 0.085,
                power,
            });
        }
    }
    /// A paused source freezes the scene. A missing/new sample decays instead of
    /// inventing beats. All particles and blast trains have fixed upper bounds.
    pub fn advance(&mut self, frame: Option<AnalysisFrame>, dt: f32, playing: bool) {
        if !playing {
            return;
        }
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        self.available = true;
        self.elapsed += dt;
        self.stale += dt;
        let fresh = frame.filter(|frame| frame.sequence != self.sequence && frame.sequence != 0);
        let mut beat = false;
        if let Some(frame) = fresh {
            self.sequence = frame.sequence;
            let sample_dt = self.stale;
            self.stale = 0.0;
            beat = frame.beats != self.beats && frame.beats != 0;
            self.beats = frame.beats;
            let smooth = |current: f32, target: f32| {
                let seconds = if target > current { 0.025 } else { 0.14 };
                current + (target - current) * (1.0 - (-sample_dt / seconds).exp())
            };
            self.level = smooth(self.level, (frame.rms * 2.2).clamp(0.0, 1.0));
            self.bass = smooth(self.bass, (frame.bass * 2.5).clamp(0.0, 1.0));
            self.treble = smooth(self.treble, (frame.treble * 3.0).clamp(0.0, 1.0));
            for (value, amplitude) in self.bands.iter_mut().zip(frame.bands) {
                let target = ((amplitude.max(0.0001).log10() * 20.0 + 60.0) / 60.0).clamp(0.0, 1.0);
                *value = smooth(*value, target);
            }
            self.wave = frame.wave;
        }
        if self.stale > 0.12 {
            let decay = (-dt * 8.0).exp();
            self.level *= decay;
            self.bass *= decay;
            self.treble *= decay;
            for band in &mut self.bands {
                *band *= decay;
            }
            for sample in &mut self.wave {
                *sample *= decay;
            }
        }
        self.flight += dt * (0.05 + self.level * 0.06);
        self.flash *= (-dt * 5.0).exp();
        for particle in &mut self.particles {
            particle.age += dt;
            particle.x += particle.vx * dt;
            particle.y += particle.vy * dt;
            particle.vx *= (-dt * 0.5).exp();
            particle.vy *= (-dt * 0.5).exp();
        }
        self.particles
            .retain(|p| p.age < 2.4 && (-0.1..1.1).contains(&p.x) && (-0.1..1.1).contains(&p.y));
        for pulse in &mut self.pulses {
            pulse.age += dt;
        }
        self.pulses.retain(|pulse| pulse.age < 2.2);
        for impact in &mut self.impacts {
            impact.age += dt;
        }
        self.impacts.retain(|impact| impact.age < 3.2);
        for bomb in &mut self.bombs {
            bomb.age += dt;
            if bomb.age >= 1.1 && self.impacts.len() < 48 {
                self.impacts.push(Impact {
                    x: bomb.target_x,
                    y: bomb.target_y,
                    age: 0.0,
                    power: bomb.power,
                });
            }
        }
        self.bombs.retain(|bomb| bomb.age < 1.1);
        if beat && self.level > 0.015 {
            let power = (self.level + self.bass * 0.5).clamp(0.15, 1.0);
            self.flash = power;
            if self.pulses.len() < 12 {
                self.pulses.push(Pulse { age: 0.0, power });
            }
            self.burst(power, 24 + (self.treble * 36.0) as usize);
            self.drop_salvo(power);
            self.salvo = 0.0;
        }
        if self.level > 0.025 {
            self.emission += dt * (8.0 + self.treble * 36.0);
            let count = self.emission as usize;
            self.emission -= count as f32;
            self.burst(self.level, count);
            self.salvo += dt * (0.5 + self.level + self.bass);
            if self.salvo >= 1.0 {
                self.salvo -= 1.0;
                self.drop_salvo(self.level);
            }
        } else {
            self.salvo = 0.0;
            self.emission = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn music(sequence: u64, beats: u64) -> AnalysisFrame {
        AnalysisFrame {
            sequence,
            beats,
            rms: 0.4,
            bass: 0.3,
            treble: 0.15,
            bands: [0.2; SPECTRUM_BANDS],
            wave: [0.4; WAVE_SAMPLES],
        }
    }
    #[test]
    fn music_creates_effects_pause_freezes_and_silence_fades() {
        let mut state = VisualizerState::default();
        state.advance(Some(music(1, 1)), 0.02, true);
        assert!(!state.bombs.is_empty() && !state.particles.is_empty() && !state.pulses.is_empty());
        let elapsed = state.elapsed;
        let bombs = state.bombs[0].age;
        state.advance(Some(music(2, 2)), 0.1, false);
        assert_eq!(state.elapsed, elapsed);
        assert_eq!(state.bombs[0].age, bombs);
        for sequence in 2..100 {
            state.advance(
                Some(AnalysisFrame {
                    sequence,
                    beats: 1,
                    ..Default::default()
                }),
                0.1,
                true,
            );
        }
        assert!(state.bombs.is_empty() && state.impacts.is_empty() && state.particles.is_empty());
        assert!(state.level < 0.001 && state.bands.iter().all(|v| *v < 0.001));
        state.reset();
        assert!(!state.available);
    }
    #[test]
    fn repeated_snapshot_does_not_create_extra_beats_and_effects_are_bounded() {
        let mut state = VisualizerState::default();
        for sequence in 1..2000 {
            state.advance(Some(music(sequence, sequence)), 0.01, true);
            assert!(
                state.bombs.len() <= 64
                    && state.impacts.len() <= 48
                    && state.particles.len() <= 192
                    && state.pulses.len() <= 12
            );
        }
        state.reset();
        for _ in 0..200 {
            state.advance(Some(music(1, 1)), 0.02, true);
        }
        assert!(state.level < 0.001 && state.bombs.is_empty());
    }
}
