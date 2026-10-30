use super::{
    App,
    background::{LatestJob, background},
};
use crate::{
    preferences::NamedPalette,
    wallpaper_palette::{ImageStamp, Source, WallpaperSettings},
};
use anyhow::{Result, ensure};
use std::time::{Duration, Instant};

type Request = (WallpaperSettings, Option<ImageStamp>);

#[derive(Default)]
pub(super) struct WallpaperPaletteState {
    settings: Option<WallpaperSettings>,
    stamp: Option<ImageStamp>,
    palette: Option<NamedPalette>,
    error: Option<String>,
    job: LatestJob<Request, (ImageStamp, Option<NamedPalette>)>,
    next: Option<Instant>,
}

impl App {
    pub(crate) fn wallpaper_palette_ready(&self) -> bool {
        self.wallpaper_palette.settings.as_ref()
            == Some(&self.settings.preferences.appearance.wallpaper_palette)
            && self.wallpaper_palette.palette.is_some()
            && self
                .settings
                .preferences
                .appearance
                .wallpaper_palette
                .source
                != Source::Palette
    }
    pub fn current_palette(&self) -> &NamedPalette {
        let options = &self.settings.preferences.appearance.wallpaper_palette;
        if options.source != Source::Palette
            && self.wallpaper_palette.settings.as_ref() == Some(options)
            && let Some(palette) = &self.wallpaper_palette.palette
        {
            return palette;
        }
        self.settings.current_palette()
    }

    pub(crate) fn wallpaper_palette_status(&self) -> String {
        if self
            .settings
            .preferences
            .appearance
            .wallpaper_palette
            .source
            == Source::Palette
        {
            return self.text("Selected palette", "Выбранная палитра").into();
        }
        if self.wallpaper_palette.job.requested().is_some() {
            return self.text("Preparing…", "Подготовка…").into();
        }
        if let Some(error) = &self.wallpaper_palette.error {
            return error.clone();
        }
        if self.wallpaper_palette.palette.is_some() {
            self.text("Ready", "Готово").into()
        } else {
            self.text("Choose a source", "Выберите источник").into()
        }
    }

    pub(super) fn save_wallpaper_palette(&mut self) -> Result<()> {
        let options = &self.settings.preferences.appearance.wallpaper_palette;
        ensure!(
            self.wallpaper_palette.settings.as_ref() == Some(options),
            self.text("Wait for the palette", "Дождитесь создания палитры")
        );
        let mut palette = self.wallpaper_palette.palette.clone().ok_or_else(|| {
            anyhow::anyhow!(self.text("Choose an image first", "Сначала выберите изображение"))
        })?;
        palette.id = self.settings.fresh_id("palette");
        palette.name = self.text("Wallpaper colors", "Цвета обоев").into();
        let mut suffix = 2;
        let name = palette.name.clone();
        while self
            .settings
            .palettes
            .iter()
            .any(|existing| existing.name == palette.name)
        {
            palette.name = format!("{name} ({suffix})");
            suffix += 1;
        }
        self.settings.palettes.push(palette);
        self.view.settings.palette = self.settings.palettes.len() - 1;
        self.save_settings()?;
        self.message(self.text("Palette saved", "Палитра сохранена").into());
        Ok(())
    }

    pub(super) fn tick_wallpaper_palette(&mut self) {
        let options = self
            .settings
            .preferences
            .appearance
            .wallpaper_palette
            .clone();
        if self.wallpaper_palette.settings.as_ref() != Some(&options) {
            self.wallpaper_palette.settings = Some(options.clone());
            self.wallpaper_palette.stamp = None;
            self.wallpaper_palette.palette = None;
            self.wallpaper_palette.error = None;
            self.wallpaper_palette.next = None;
            self.wallpaper_palette.job.cancel();
        }
        if let Some(completed) = self.wallpaper_palette.job.poll()
            && completed.current
        {
            match completed.result {
                Ok((stamp, palette)) => {
                    self.wallpaper_palette.stamp = Some(stamp);
                    if let Some(palette) = palette {
                        self.wallpaper_palette.palette = Some(palette);
                    }
                    self.wallpaper_palette.error = None;
                }
                Err(error) => {
                    self.wallpaper_palette.error = Some(error.to_string());
                }
            }
            self.wallpaper_palette.next = Some(Instant::now() + Duration::from_secs(5));
        }
        if options.source == Source::Palette
            || self.wallpaper_palette.job.is_running()
            || self
                .wallpaper_palette
                .next
                .is_some_and(|next| Instant::now() < next)
        {
            return;
        }
        self.wallpaper_palette
            .job
            .request((options, self.wallpaper_palette.stamp.clone()));
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        let language = self.settings.preferences.language;
        self.wallpaper_palette.job.start(|(options, stamp)| {
            let options = options.clone();
            let stamp = stamp.clone();
            background(runtime, move || {
                crate::wallpaper_palette::refresh(&options, stamp.as_ref(), language)
            })
        });
    }
}
