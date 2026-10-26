use super::{
    App,
    background::{LatestJob, background},
};
use crate::{background_gif::GifAnimation, visualizer::VisualizerKind};
use std::{path::PathBuf, sync::Arc, time::Instant};

#[derive(Default)]
pub(super) struct GifBackgroundState {
    source: Option<PathBuf>,
    animation: Option<Arc<GifAnimation>>,
    started: Option<Instant>,
    job: LatestJob<PathBuf, GifAnimation>,
    error: Option<String>,
}

impl App {
    pub fn drag_gif_opacity(
        &mut self,
        index: usize,
        area: ratatui::layout::Rect,
        ratio: f64,
        release: bool,
    ) {
        if !ratio.is_finite()
            || area.is_empty()
            || !matches!(self.view.dialog, Some(super::Dialog::Settings { .. }))
        {
            return;
        }
        if !self
            .settings_rows()
            .get(index)
            .is_some_and(|row| matches!(row.control, super::SettingControl::GifOpacity))
        {
            return;
        }
        self.select_setting(index);
        self.view.workspace.gesture = if release {
            None
        } else {
            Some(crate::workspace::Gesture::GifOpacity(index, area))
        };
        let opacity = (ratio.clamp(0.0, 1.0) * 30.0).round() as u8;
        if opacity != self.settings.preferences.appearance.gif_background.opacity {
            self.settings.preferences.appearance.gif_background.opacity = opacity;
            if let Err(error) = self.save_settings() {
                self.failure(error);
            }
        }
    }

    pub fn background_gif_frame(&self) -> Option<(&Arc<GifAnimation>, usize)> {
        let settings = &self.settings.preferences.appearance;
        if settings.visualizer != VisualizerKind::Gif
            || settings.gif_background.opacity.min(30) == 0
            || settings.gif_background.path != self.gif_background.source
        {
            return None;
        }
        let animation = self.gif_background.animation.as_ref()?;
        Some((
            animation,
            animation.frame_at(self.gif_background.started?.elapsed()),
        ))
    }

    pub(crate) fn background_gif_status(&self) -> String {
        if self.gif_background.job.requested().is_some() {
            return self.text("Loading…", "Загрузка…").into();
        }
        if let Some(error) = &self.gif_background.error {
            return error.clone();
        }
        self.settings
            .preferences
            .appearance
            .gif_background
            .path
            .as_ref()
            .map_or_else(
                || self.text("Choose file…", "Выбрать файл…").into(),
                |path| path.display().to_string(),
            )
    }

    pub(super) fn reload_background_gif(&mut self) {
        self.gif_background.source = None;
        self.gif_background.error = None;
        self.gif_background.animation = None;
        self.gif_background.job.cancel();
    }

    pub(super) fn tick_gif_background(&mut self) {
        let appearance = &self.settings.preferences.appearance;
        let source = (appearance.visualizer == VisualizerKind::Gif)
            .then(|| appearance.gif_background.path.clone())
            .flatten();
        if source != self.gif_background.source {
            self.gif_background.source = source.clone();
            self.gif_background.animation = None;
            self.gif_background.error = None;
            if let Some(path) = source {
                self.gif_background.job.request(path);
            } else {
                self.gif_background.job.cancel();
            }
        }
        self.start_gif_background_job();
        if let Some(completed) = self.gif_background.job.poll() {
            if completed.current {
                match completed.result {
                    Ok(animation) => {
                        self.gif_background.animation = Some(Arc::new(animation));
                        self.gif_background.started = Some(Instant::now());
                    }
                    Err(error) => {
                        let message = format!(
                            "{}: {error:#}",
                            self.text("GIF background unavailable", "GIF-фон недоступен")
                        );
                        self.gif_background.error = Some(message.clone());
                        self.message(message);
                        self.view.notice_error = true;
                    }
                }
            }
            self.start_gif_background_job();
        }
    }

    fn start_gif_background_job(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        let language = self.settings.preferences.language;
        self.gif_background.job.start(|path| {
            let path = path.clone();
            background(runtime, move || {
                crate::background_gif::read(&path, language)
            })
        });
    }
}
