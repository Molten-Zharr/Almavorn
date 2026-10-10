use super::App;
use crate::visualizer::VisualizerKind;
use anyhow::Result;
use std::time::Instant;

impl App {
    pub fn cycle_visualizer(&mut self, direction: i64) -> Result<()> {
        self.set_visualizer(self.settings.appearance.visualizer.next(direction))
    }

    pub fn set_visualizer(&mut self, kind: VisualizerKind) -> Result<()> {
        self.settings.appearance.visualizer = kind;
        self.tick_visualizer();
        self.message(format!(
            "{}: {}",
            self.text("Background animation", "Анимация фона"),
            self.settings
                .appearance
                .visualizer
                .name(self.settings.language)
        ));
        self.save_settings()
    }
    pub(super) fn tick_visualizer(&mut self) {
        let now = Instant::now();
        let dt = self
            .playback
            .visualizer_tick
            .replace(now)
            .map_or(0.0, |previous| now.duration_since(previous).as_secs_f32());
        let enabled = self.settings.appearance.visualizer != VisualizerKind::Off;
        let Some(audio) = &mut self.playback.audio else {
            self.playback.visualizer.reset();
            return;
        };
        audio.enable_analysis(enabled);
        if self.playback.current.is_none() || !audio.has_track() || !enabled {
            self.playback.visualizer.reset();
            return;
        }
        let generation = audio.analysis_generation();
        if generation != self.playback.visualizer_generation {
            self.playback.visualizer.reset();
            self.playback.visualizer_generation = generation;
        }
        let playing = !audio.paused() && self.playback.seek_preview.is_none();
        self.playback
            .visualizer
            .advance(audio.analysis(), dt, playing);
    }
}
