use crate::model::Language;

#[derive(Clone, Copy, Debug)]
pub struct Hsv {
    pub hue: f64,
    pub saturation: f64,
    pub value: f64,
}

impl Hsv {
    pub fn from_rgb(rgb: [u8; 3]) -> Self {
        let [r, g, b] = rgb.map(|v| f64::from(v) / 255.0);
        let maximum = r.max(g).max(b);
        let minimum = r.min(g).min(b);
        let range = maximum - minimum;
        let hue = if range == 0.0 {
            0.0
        } else if maximum == r {
            ((g - b) / range).rem_euclid(6.0) * 60.0
        } else if maximum == g {
            ((b - r) / range + 2.0) * 60.0
        } else {
            ((r - g) / range + 4.0) * 60.0
        };
        Self {
            hue,
            saturation: if maximum == 0.0 { 0.0 } else { range / maximum },
            value: maximum,
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        let hue = self.hue.rem_euclid(360.0) / 60.0;
        let chroma = self.value * self.saturation;
        let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
        let components = match hue as u8 {
            0 => [chroma, x, 0.0],
            1 => [x, chroma, 0.0],
            2 => [0.0, chroma, x],
            3 => [0.0, x, chroma],
            4 => [x, 0.0, chroma],
            _ => [chroma, 0.0, x],
        };
        components.map(|component| {
            ((component + self.value - chroma).clamp(0.0, 1.0) * 255.0).round() as u8
        })
    }
}

#[derive(Clone, Debug)]
pub struct ColorPicker {
    pub hsv: Hsv,
    pub original: [u8; 3],
    /// None selects the HEX field; 0..6 selects H, S, V, R, G, B.
    pub selected: Option<usize>,
}

impl ColorPicker {
    pub const CHANNELS: usize = 6;

    pub fn new(rgb: [u8; 3]) -> Self {
        Self {
            hsv: Hsv::from_rgb(rgb),
            original: rgb,
            selected: None,
        }
    }

    pub fn set_rgb(&mut self, rgb: [u8; 3]) {
        let mut hsv = Hsv::from_rgb(rgb);
        if hsv.saturation == 0.0 {
            hsv.hue = self.hsv.hue;
        }
        self.hsv = hsv;
    }

    pub fn maximum(index: usize) -> f64 {
        match index {
            0 => 359.0,
            1 | 2 => 100.0,
            _ => 255.0,
        }
    }

    pub fn channel(&self, index: usize) -> f64 {
        match index {
            0 => self.hsv.hue,
            1 => self.hsv.saturation * 100.0,
            2 => self.hsv.value * 100.0,
            3..=5 => f64::from(self.hsv.rgb()[index - 3]),
            _ => 0.0,
        }
    }

    pub fn set_channel(&mut self, index: usize, value: f64) {
        let value = value.clamp(0.0, Self::maximum(index));
        match index {
            0 => self.hsv.hue = value,
            1 => self.hsv.saturation = value / 100.0,
            2 => self.hsv.value = value / 100.0,
            3..=5 => {
                let mut rgb = self.hsv.rgb();
                rgb[index - 3] = value.round() as u8;
                self.set_rgb(rgb);
            }
            _ => {}
        }
    }

    pub fn pick_wheel(&mut self, x: f64, y: f64) {
        let radius = x.hypot(y);
        if radius > f64::EPSILON {
            self.hsv.hue = y.atan2(x).to_degrees().rem_euclid(360.0);
        }
        self.hsv.saturation = radius.min(1.0);
    }

    pub fn wheel_rgb(x: f64, y: f64) -> Option<[u8; 3]> {
        let radius = x.hypot(y);
        (radius <= 1.0).then(|| {
            Hsv {
                hue: y.atan2(x).to_degrees().rem_euclid(360.0),
                saturation: radius,
                value: 1.0,
            }
            .rgb()
        })
    }

    pub fn gradient(&self, index: usize, ratio: f64) -> [u8; 3] {
        let mut color = self.clone();
        if index == 0 {
            color.hsv.saturation = 1.0;
            color.hsv.value = 1.0;
        }
        color.set_channel(index, ratio * Self::maximum(index));
        color.hsv.rgb()
    }

    pub fn label(index: usize, language: Language, compact: bool) -> &'static str {
        match (index, compact) {
            (0, false) => language.text("Hue (H)", "Оттенок (H)"),
            (1, false) => language.text("Saturation (S)", "Насыщенность (S)"),
            (2, false) => language.text("Brightness (V)", "Яркость (V)"),
            (3, false) => language.text("Red (R)", "Красный (R)"),
            (4, false) => language.text("Green (G)", "Зелёный (G)"),
            (5, false) => language.text("Blue (B)", "Синий (B)"),
            (0, true) => "H",
            (1, true) => "S",
            (2, true) => "V",
            (3, true) => "R",
            (4, true) => "G",
            _ => "B",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_hsv_roundtrip_preserves_palette_colors() {
        for red in (0..=255).step_by(17) {
            for green in (0..=255).step_by(17) {
                for blue in (0..=255).step_by(17) {
                    let rgb = [red, green, blue];
                    assert_eq!(Hsv::from_rgb(rgb).rgb(), rgb);
                }
            }
        }
        assert_eq!(ColorPicker::wheel_rgb(1.0, 0.0), Some([255, 0, 0]));
        assert_eq!(ColorPicker::wheel_rgb(0.0, 0.0), Some([255, 255, 255]));
        assert_eq!(ColorPicker::wheel_rgb(1.0, 1.0), None);
        let mut color = ColorPicker::new([0, 0, 0]);
        color.pick_wheel(-0.5, 3.0f64.sqrt() / 2.0);
        color.set_channel(2, 100.0);
        assert_eq!(color.hsv.rgb(), [0, 255, 0]);
    }
}
