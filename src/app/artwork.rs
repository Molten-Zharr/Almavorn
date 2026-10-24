use super::{
    App,
    background::{LatestJob, background},
};
use crate::artwork::{ArtworkImage, ArtworkRequest};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct ArtworkState {
    image: Option<Arc<ArtworkImage>>,
    source: Option<ArtworkRequest>,
    job: LatestJob<ArtworkRequest, Option<ArtworkImage>>,
}

impl App {
    pub fn artwork_image(&self) -> Option<&Arc<ArtworkImage>> {
        self.artwork.image.as_ref()
    }

    pub(super) fn invalidate_artwork(&mut self) {
        self.artwork.source = None;
        self.artwork.image = None;
        self.artwork.job.cancel();
    }

    pub(super) fn tick_artwork(&mut self) {
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
}
