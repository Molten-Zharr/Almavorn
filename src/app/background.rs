use crate::errors::AppError;
use anyhow::{Context, Result};
use tokio::{runtime::Runtime, sync::oneshot};

pub(super) type Background<T> = oneshot::Receiver<Result<T>>;

pub(super) fn background<T: Send + 'static>(
    runtime: &Runtime,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Background<T> {
    let (sender, receiver) = oneshot::channel();
    runtime.spawn(async move {
        let result = tokio::task::spawn_blocking(operation)
            .await
            .context(AppError::BackgroundFailed)
            .and_then(|result| result);
        let _ = sender.send(result);
    });
    receiver
}

pub(super) fn poll<T>(receiver: &mut Background<T>) -> Option<Result<T>> {
    match receiver.try_recv() {
        Ok(result) => Some(result),
        Err(oneshot::error::TryRecvError::Closed) => {
            Some(Err(AppError::BackgroundInterrupted.into()))
        }
        Err(oneshot::error::TryRecvError::Empty) => None,
    }
}

struct Running<R, T> {
    request: R,
    receiver: Background<T>,
    generation: u64,
}

pub(super) struct Completion<R, T> {
    pub request: R,
    pub result: Result<T>,
    pub current: bool,
}

/// One running blocking operation and the latest pending request. Canceling
/// invalidates results; it does not pretend that blocking I/O can be aborted.
pub(super) struct LatestJob<R, T> {
    running: Option<Running<R, T>>,
    pending: Option<R>,
    generation: u64,
}

impl<R, T> Default for LatestJob<R, T> {
    fn default() -> Self {
        Self {
            running: None,
            pending: None,
            generation: 0,
        }
    }
}

impl<R, T> LatestJob<R, T> {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn request(&mut self, request: R) {
        self.cancel();
        self.pending = Some(request);
    }

    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
    }

    pub fn requested(&self) -> Option<&R> {
        self.pending.as_ref().or_else(|| {
            self.running
                .as_ref()
                .filter(|job| job.generation == self.generation)
                .map(|job| &job.request)
        })
    }

    pub fn requested_mut(&mut self) -> Option<&mut R> {
        let generation = self.generation;
        self.pending.as_mut().or_else(|| {
            self.running
                .as_mut()
                .filter(|job| job.generation == generation)
                .map(|job| &mut job.request)
        })
    }

    pub fn start(&mut self, spawn: impl FnOnce(&R) -> Background<T>) {
        if self.running.is_none()
            && let Some(request) = self.pending.take()
        {
            let receiver = spawn(&request);
            self.running = Some(Running {
                request,
                receiver,
                generation: self.generation,
            });
        }
    }

    pub fn poll(&mut self) -> Option<Completion<R, T>> {
        let result = poll(&mut self.running.as_mut()?.receiver)?;
        let job = self.running.take().expect("completed job exists");
        Some(Completion {
            request: job.request,
            result,
            current: job.generation == self.generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_request_cannot_overwrite_the_latest_request() {
        let mut job = LatestJob::<u8, u8>::default();
        let (sender, receiver) = oneshot::channel();
        job.request(1);
        job.start(|_| receiver);
        job.request(2);
        job.request(3);
        assert_eq!(job.requested(), Some(&3));
        let mut launched = false;
        job.start(|_| {
            launched = true;
            unreachable!()
        });
        assert!(!launched);
        sender.send(Ok(1)).unwrap();
        assert!(!job.poll().unwrap().current);
        job.start(|request| {
            let (sender, receiver) = oneshot::channel();
            sender.send(Ok(*request)).unwrap();
            receiver
        });
        let completed = job.poll().unwrap();
        assert!(completed.current);
        assert_eq!(completed.request, 3);
        assert_eq!(completed.result.unwrap(), 3);
        assert!(job.requested().is_none());
    }

    #[test]
    fn cancellation_invalidates_an_already_running_result() {
        let mut job = LatestJob::<bool, ()>::default();
        let (sender, receiver) = oneshot::channel();
        job.request(false);
        job.start(|_| receiver);
        *job.requested_mut().unwrap() = true;
        job.cancel();
        assert!(job.requested().is_none());
        sender.send(Ok(())).unwrap();
        assert!(!job.poll().unwrap().current);
        assert!(job.poll().is_none());
    }

    #[test]
    fn closed_channel_reports_an_error_and_releases_the_worker_slot() {
        let mut job = LatestJob::<(), ()>::default();
        job.request(());
        job.start(|_| {
            let (sender, receiver) = oneshot::channel();
            drop(sender);
            receiver
        });
        let error = job.poll().unwrap().result.unwrap_err();
        assert_eq!(
            error.downcast_ref::<AppError>(),
            Some(&AppError::BackgroundInterrupted)
        );
        assert!(job.poll().is_none());
        job.request(());
        job.start(|_| {
            let (sender, receiver) = oneshot::channel();
            sender.send(Ok(())).unwrap();
            receiver
        });
        assert!(job.poll().unwrap().result.is_ok());
    }
}
