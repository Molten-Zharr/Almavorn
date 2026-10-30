use super::{
    App,
    background::{LatestJob, background},
};
use crate::{
    artwork::{ArtworkImage, ArtworkRequest},
    errors::AppError,
};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
};

#[derive(Default)]
pub(super) struct ArtworkState {
    image: Option<Arc<ArtworkImage>>,
    source: Option<ArtworkRequest>,
    job: LatestJob<ArtworkRequest, Option<ArtworkImage>>,
    opening: LatestJob<ArtworkRequest, PathBuf>,
    viewers: Vec<Child>,

}

impl App {
    pub fn artwork_image(&self) -> Option<&Arc<ArtworkImage>> {
        self.artwork.image.as_ref()
    }

    pub(crate) fn opening_artwork(&self) -> bool {
        self.artwork.opening.requested().is_some()
    }

    pub(super) fn open_artwork(&mut self) {
        if self.artwork.image.is_some()
            && let Some(source) = self.artwork.source.clone()
        {
            self.artwork.opening.request(source);
        }
    }

    pub(super) fn invalidate_artwork(&mut self) {
        self.artwork.source = None;
        self.artwork.image = None;
        self.artwork.job.cancel();
    }

    pub(super) fn tick_artwork(&mut self) {
        self.tick_artwork_opening();
        let settings = &self.settings.preferences.artwork;
        let source = self
            .playback
            .current
            .as_ref()
            .filter(|_| settings.visible)
            .map(|track| {
                if settings.shared
                    && let Some(path) = &settings.path
                {
                    return ArtworkRequest::Shared(path.clone());
                }
                ArtworkRequest::Embedded(track.path.clone())
            });
        if source != self.artwork.source {
            self.artwork.source = source.clone();
            self.artwork.image = None;
            if let Some(request) = source {
                self.artwork.job.request(request);
            } else {
                self.artwork.job.cancel();
            }
        }
        self.start_artwork_job();
        if let Some(completed) = self.artwork.job.poll() {
            if completed.current {
                match completed.result {
                    Ok(image) => self.artwork.image = image.map(Arc::new),
                    Err(error) => {
                        let reason =
                            crate::errors::message(&error, self.settings.preferences.language);
                        self.message(format!(
                            "{}: {reason}",
                            self.text("Artwork unavailable", "Обложка недоступна")
                        ));
                        self.view.notice_error = true;
                    }
                }
            }
            self.start_artwork_job();
        }
    }

    fn start_artwork_job(&mut self) {
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.artwork.job.start(|request| {
            let request = request.clone();
            background(runtime, move || crate::artwork::read(&request))
        });
    }

    fn tick_artwork_opening(&mut self) {
        let directory = self.store.path.parent().expect("library directory");
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.artwork.opening.start(|request| {
            let request = request.clone();
            let cache = directory.join("artwork-cache");
            background(runtime, move || crate::artwork::original_path(&request, &cache))
        });
        if let Some(completed) = self.artwork.opening.poll()
            && completed.current
        {
            match completed.result {
                Ok(path) => {
                    match Command::new("xdg-open")
                        .arg(path)
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                    {
                        Ok(child) => self.artwork.viewers.push(child),
                        Err(error) => self.failure(anyhow::Error::new(error).context(AppError::ArtworkOpenFailed)),
                    }
                }
                Err(error) => self.failure(error),
            }
        }
        let mut failed = false;
        self.artwork.viewers.retain_mut(|child| match child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) if status.success() => false,
            _ => {
                failed = true;
                false
            }
        });
        if failed {
            self.failure(AppError::ArtworkOpenFailed.into());
        }
    }
}
