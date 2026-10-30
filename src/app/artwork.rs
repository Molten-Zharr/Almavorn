use super::{
    App,
    background::{LatestJob, background},
};
use crate::{
    artwork::{ArtworkImage, ArtworkRequest},
    errors::AppError,
    model::Track,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct ArtworkState {
    image: Option<Arc<ArtworkImage>>,
    source: Option<ArtworkRequest>,
    job: LatestJob<ArtworkRequest, Option<ArtworkImage>>,
    opening: LatestJob<ArtworkRequest, PathBuf>,
    viewers: Vec<Child>,
    search_job: LatestJob<Track, bool>,
    search_enabled: Option<Arc<Mutex<bool>>>,
    search_queue: VecDeque<Track>,
    search_seen: HashMap<PathBuf, (String, Instant)>,
    search_refresh: Option<Instant>,
    search_next: Option<Instant>,
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
        self.tick_artwork_search();
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

    pub(super) fn cancel_artwork_search(&mut self) {
        if let Some(enabled) = self.artwork.search_enabled.take() {
            *enabled.lock().unwrap_or_else(|error| error.into_inner()) = false;
        }
        self.artwork.search_job.cancel();
        self.artwork.search_queue.clear();
        self.artwork.search_seen.clear();
        self.artwork.search_refresh = None;
    }

    fn tick_artwork_search(&mut self) {
        if let Some(completed) = self.artwork.search_job.poll() {
            if completed.current && completed.result.is_ok()
                && let Some((_, until)) = self.artwork.search_seen.get_mut(&completed.request.path)
            {
                *until = Instant::now() + Duration::from_secs(24 * 60 * 60);
            }
            if completed.current && completed.result.as_ref().is_ok_and(|saved| *saved)
                && self.playback.current.as_ref().is_some_and(|track| track.path == completed.request.path)
            {
                self.invalidate_artwork();
            }
            self.artwork.search_next = Some(Instant::now() + Duration::from_millis(1200));
        }
        if !self.settings.preferences.artwork.search_online {
            if self.artwork.search_enabled.is_some() { self.cancel_artwork_search(); }
            return;
        }
        let enabled = self.artwork.search_enabled.get_or_insert_with(|| Arc::new(Mutex::new(true))).clone();
        let now = Instant::now();
        if self.artwork.search_queue.is_empty() && self.artwork.search_refresh.is_none_or(|at| now >= at) {
            let mut unique = HashSet::new();
            let tracks: Vec<_> = self.playlists().iter().flat_map(|playlist| &playlist.entries)
                .map(|entry| &entry.track)
                .filter(|track| unique.insert(track.path.clone()))
                .filter(|track| {
                    let key = format!("{}\0{}\0{}\0{}", track.artist, track.title, track.album, track.duration_ms);
                    self.artwork.search_seen.get(&track.path).is_none_or(|(old, until)| *old != key || now >= *until)
                }).cloned().collect();
            self.artwork.search_queue.extend(tracks);
            self.artwork.search_refresh = Some(now + Duration::from_secs(60));
        }
        if self.artwork.search_job.is_running() || self.artwork.search_next.is_some_and(|at| now < at) { return; }
        let Some(track) = self.artwork.search_queue.pop_front() else { return; };
        let key = format!("{}\0{}\0{}\0{}", track.artist, track.title, track.album, track.duration_ms);
        self.artwork.search_seen.insert(track.path.clone(), (key, now + Duration::from_secs(15 * 60)));
        self.artwork.search_job.request(track);
        let cache = self.store.path.parent().expect("library directory").join("cover-search-cache");
        let runtime = self.runtime.as_ref().expect("runtime is alive");
        self.artwork.search_job.start(|track| {
            let track = track.clone();
            background(runtime, move || crate::artwork::search::find_and_save(&track, &cache, &enabled))
        });
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
