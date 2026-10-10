use super::theme::Palette;
use crate::{
    app::App,
    visualizer::{VisualizerKind, VisualizerState},
};
use ratatui::{Frame, buffer::CellDiffOption, layout::Rect, style::Color};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Default)]
struct DotCell {
    bits: u8,
    rgb: [u8; 3],
    strength: f32,
}
struct Canvas {
    area: Rect,
    cells: Vec<DotCell>,
    width: f32,
    height: f32,
}
impl Canvas {
    fn new(area: Rect) -> Self {
        Self {
            area,
            cells: vec![DotCell::default(); usize::from(area.width) * usize::from(area.height)],
            width: f32::from(area.width) * 2.0,
            height: f32::from(area.height) * 4.0,
        }
    }
    fn dot(&mut self, x: f32, y: f32, rgb: [u8; 3], strength: f32) {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= self.width
            || y >= self.height
        {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        let index = (y / 4) * usize::from(self.area.width) + x / 2;
        let cell = &mut self.cells[index];
        cell.bits |= 1 << [[0, 1, 2, 6], [3, 4, 5, 7]][x % 2][y % 4];
        if strength > cell.strength {
            cell.strength = strength;
            cell.rgb = rgb;
        }
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32), rgb: [u8; 3], strength: f32) {
        let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize)
            .clamp(1, (self.width + self.height) as usize * 2 + 1);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            self.dot(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, rgb, strength);
        }
    }
    fn circle(&mut self, center: (f32, f32), radius: (f32, f32), rgb: [u8; 3], strength: f32) {
        let steps = (radius.0.max(radius.1) * TAU).ceil() as usize;
        for i in 0..steps.clamp(16, (self.width + self.height) as usize * 4 + 16) {
            let angle = TAU * i as f32 / steps.max(16) as f32;
            self.dot(
                center.0 + angle.cos() * radius.0,
                center.1 + angle.sin() * radius.1,
                rgb,
                strength,
            );
        }
    }
    fn spectrum(&mut self, state: &VisualizerState, accent: [u8; 3], hot: [u8; 3]) {
        let count = state.bands.len();
        let step = self.width / count as f32;
        for (index, band) in state.bands.iter().enumerate() {
            let height = band * self.height * 0.68;
            let left = index as f32 * step;
            for x in (left as usize)..((left + step * 0.72) as usize) {
                self.line(
                    (x as f32, self.height - 2.0),
                    (x as f32, self.height - height - 2.0),
                    mix(accent, hot, index as f32 / count as f32),
                    0.55 + band * 0.4,
                );
            }
        }
    }
    fn wave(&mut self, state: &VisualizerState, accent: [u8; 3]) {
        for layer in 0..3 {
            let center = self.height * (0.3 + layer as f32 * 0.2);
            let amplitude = self.height * (0.1 + state.bass * 0.08);
            for i in 1..state.wave.len() {
                let x = |index: usize| {
                    index as f32 * (self.width - 1.0) / (state.wave.len() - 1) as f32
                };
                let y = |index: usize| {
                    center
                        - state.wave[index].clamp(-1.0, 1.0)
                            * amplitude
                            * (1.0 - layer as f32 * 0.22)
                };
                self.line(
                    (x(i - 1), y(i - 1)),
                    (x(i), y(i)),
                    accent,
                    0.9 - layer as f32 * 0.22,
                );
            }
        }
    }
    fn rings(&mut self, state: &VisualizerState, accent: [u8; 3], hot: [u8; 3]) {
        let center = (self.width * 0.5, self.height * 0.5);
        let scale = self.width.min(self.height);
        let radius = scale * (0.07 + state.level * 0.12 + state.flash * 0.04);
        self.circle(center, (radius, radius), hot, 0.9);
        for pulse in &state.pulses {
            let radius = scale * (0.1 + pulse.age * 0.28);
            self.circle(
                center,
                (radius, radius),
                accent,
                (1.0 - pulse.age / 2.2) * pulse.power,
            );
        }
        for (i, band) in state.bands.iter().enumerate() {
            let angle = TAU * i as f32 / state.bands.len() as f32 + state.elapsed * 0.16;
            let outer = radius + band * scale * 0.15;
            self.line(
                (
                    center.0 + angle.cos() * radius,
                    center.1 + angle.sin() * radius,
                ),
                (
                    center.0 + angle.cos() * outer,
                    center.1 + angle.sin() * outer,
                ),
                accent,
                0.75,
            );
        }
    }
    fn particles(&mut self, state: &VisualizerState, accent: [u8; 3], hot: [u8; 3]) {
        for particle in &state.particles {
            let end = (particle.x * self.width, particle.y * self.height);
            self.line(
                (
                    end.0 - particle.vx * self.width * 0.1,
                    end.1 - particle.vy * self.height * 0.1,
                ),
                end,
                mix(accent, hot, particle.power),
                (1.0 - particle.age / 2.4) * (0.5 + particle.power * 0.5),
            );
        }
    }
    fn bombardment(&mut self, state: &VisualizerState) {
        let red = [240, 57, 46];
        let ember = [255, 155, 58];
        let smoke = [166, 98, 104];
        let ground = self.height * 0.85;
        // A red city silhouette, distant searchlights and swept-wing formations.
        for index in 0..28 {
            let x = index as f32 * self.width / 28.0;
            let top = ground - self.height * (0.05 + ((index * 13 + 7) % 11) as f32 * 0.014);
            let right = x + self.width / 35.0;
            self.line((x, ground), (x, top), red, 0.3);
            self.line((x, top), (right, top), red, 0.3);
            self.line((right, top), (right, ground), red, 0.3);
            if index % 3 == 0 {
                self.line(
                    (x + 2.0, top),
                    (x + 2.0, top - self.height * 0.04),
                    red,
                    0.35,
                );
            }
            for y in (top as usize + 3..ground as usize).step_by(5) {
                self.dot(x + 2.0, y as f32, ember, 0.2);
            }
        }
        self.line((0.0, ground), (self.width, ground), red, 0.35);
        for i in 0..4 {
            let x = self.width * (0.16 + i as f32 * 0.23);
            let sweep = (state.elapsed * 0.17 + i as f32).sin() * self.width * 0.13;
            self.line((x, ground), (x + sweep, self.height * 0.35), red, 0.12);
        }
        // Monument-like central spire gives the skyline a recognizable menu silhouette.
        let x = self.width * 0.27;
        self.line((x - 5.0, ground), (x, self.height * 0.48), red, 0.5);
        self.line((x + 5.0, ground), (x, self.height * 0.48), red, 0.5);
        self.line(
            (x, self.height * 0.6),
            (x + 8.0, self.height * 0.43),
            red,
            0.55,
        );
        let star = (x + 8.0, self.height * 0.4);
        for i in 0..5 {
            let point = |index: usize| {
                let a = -TAU / 4.0 + index as f32 * TAU / 5.0;
                (star.0 + a.cos() * 5.0, star.1 + a.sin() * 5.0)
            };
            self.line(point(i), point((i + 2) % 5), red, 0.55);
        }
        for index in 0..3 {
            let (x, y) = state.plane(index);
            let (x, y) = (x * self.width, y * self.height);
            let size = (self.width / 22.0).clamp(4.0, 14.0);
            let points = [
                (-size, 0.0),
                (size, 0.0),
                (-size * 0.55, -size * 0.62),
                (size * 0.2, 0.0),
                (-size * 0.55, size * 0.62),
                (size, 0.0),
            ];
            for pair in points.windows(2) {
                self.line(
                    (x + pair[0].0, y + pair[0].1),
                    (x + pair[1].0, y + pair[1].1),
                    red,
                    0.75 + state.flash * 0.2,
                );
            }
            self.line(
                (x - size, y - size * 0.25),
                (x - size * 0.7, y + size * 0.25),
                red,
                0.8,
            );
            self.line(
                (x - size * 1.8, y),
                (x - size * 1.1, y),
                ember,
                0.25 + state.level * 0.35,
            );
        }
        for bomb in &state.bombs {
            if bomb.age < 0.0 {
                continue;
            }
            let position = |age: f32| {
                let t = (age / 1.1).clamp(0.0, 1.0);
                (
                    (bomb.x + (bomb.target_x - bomb.x) * t) * self.width,
                    (bomb.y + (bomb.target_y - bomb.y) * t * t) * self.height,
                )
            };
            let end = position(bomb.age);
            self.line(position(bomb.age - 0.07), end, ember, 0.8);
            self.dot(end.0 + 1.0, end.1, [255, 208, 108], 1.0);
        }
        for impact in &state.impacts {
            let center = (impact.x * self.width, impact.y * self.height);
            let scale = self.width.min(self.height) * (0.035 + impact.power * 0.035);
            if impact.age < 1.3 {
                let radius = scale * (0.3 + impact.age * 2.2);
                self.circle(
                    center,
                    (radius, radius * 0.55),
                    red,
                    (1.0 - impact.age / 1.3).max(0.0),
                );
                self.circle(
                    (center.0, center.1 - radius * 0.2),
                    (radius * 0.6, radius * 0.7),
                    ember,
                    (1.0 - impact.age / 1.3).max(0.0),
                );
                for i in 0..8 {
                    let angle = i as f32 * TAU / 8.0;
                    self.line(
                        center,
                        (
                            center.0 + angle.cos() * radius * 0.8,
                            center.1 + angle.sin() * radius * 0.55,
                        ),
                        ember,
                        (1.0 - impact.age * 1.5).max(0.0),
                    );
                }
            }
            let smoke_y = center.1 - impact.age * self.height * 0.033;
            let radius = scale * (0.3 + impact.age * 0.6);
            self.circle(
                (center.0 + (impact.age * 1.7).sin() * scale * 0.3, smoke_y),
                (radius, radius * 0.75),
                smoke,
                0.45 * (1.0 - impact.age / 3.2),
            );
        }
    }
}
fn rgb(color: Color) -> [u8; 3] {
    if let Color::Rgb(r, g, b) = color {
        [r, g, b]
    } else {
        [0, 0, 0]
    }
}
fn mix(a: [u8; 3], b: [u8; 3], amount: f32) -> [u8; 3] {
    std::array::from_fn(|i| {
        (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * amount.clamp(0.0, 1.0)).round()
            as u8
    })
}

/// Composite only the main background. Existing glyphs, foreground colors,
/// selections, buttons and hit targets keep their original content and priority.
pub(super) fn render(frame: &mut Frame, app: &App, palette: Palette) {
    let state = &app.playback.visualizer;
    let kind = app.settings.appearance.visualizer;
    if !state.available || kind == VisualizerKind::Off {
        return;
    }
    let mut canvas = Canvas::new(frame.area());
    let accent = rgb(palette.accent);
    let hot = rgb(palette.error);
    match kind {
        VisualizerKind::Off => return,
        VisualizerKind::Spectrum => canvas.spectrum(state, accent, hot),
        VisualizerKind::Wave => canvas.wave(state, accent),
        VisualizerKind::Rings => canvas.rings(state, accent, hot),
        VisualizerKind::Particles => canvas.particles(state, accent, hot),
        VisualizerKind::Bombardment => canvas.bombardment(state),
    }
    let background = rgb(palette.background);
    let brightness = f32::from(app.settings.appearance.animation_brightness.clamp(10, 70)) / 100.0;
    let buffer = frame.buffer_mut();
    for (index, dot) in canvas.cells.into_iter().enumerate() {
        if dot.bits == 0 || dot.strength <= 0.01 {
            continue;
        }
        let x = canvas.area.x + (index % usize::from(canvas.area.width)) as u16;
        let y = canvas.area.y + (index / usize::from(canvas.area.width)) as u16;
        let cell = &mut buffer[(x, y)];
        if cell.bg != palette.background || cell.diff_option == CellDiffOption::Skip {
            continue;
        }
        let intensity = brightness * dot.strength;
        let [r, g, b] = mix(background, dot.rgb, intensity * 0.12);
        cell.set_bg(Color::Rgb(r, g, b));
        if cell.symbol() == " " {
            cell.set_char(char::from_u32(0x2800 + u32::from(dot.bits)).unwrap_or(' '));
            let [r, g, b] = mix(background, dot.rgb, intensity);
            cell.set_fg(Color::Rgb(r, g, b));
        }
    }
}
