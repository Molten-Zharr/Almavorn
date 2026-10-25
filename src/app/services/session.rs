use super::{App, PlayRequest, Provider, Track};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Serialize, Deserialize)]
pub(in crate::app) struct ServiceSession {
    connection: String,
    queue: Vec<Track>,
    index: usize,
    position_ms: u64,
    paused: bool,
    device: Option<String>,
}

impl App {
    pub(in crate::app) fn service_session(&self) -> Option<ServiceSession> {
        if let Some(request) = self.services.play.requested()
            && (request.remote.is_none() || self.playback.current.is_none())
        {
            return Some(ServiceSession {
                connection: request.connection.id.clone(),
                queue: request.queue.clone(),
                index: request.index,
                position_ms: request.position,
                paused: request.paused,
                device: request.device.clone(),
            });
        }
        if let Some(native) = &self.services.native {
            return Some(ServiceSession {
                connection: native.connection.id.clone(),
                queue: native.queue.clone(),
                index: native.index,
                position_ms: self.position_ms(),
                paused: self.paused(),
                device: None,
            });
        }
        let connection = self.services.remote_connection.as_ref()?;
        Some(ServiceSession {
            connection: connection.id.clone(),
            queue: self
                .services
                .remote
                .as_ref()?
                .track
                .clone()
                .into_iter()
                .collect(),
            index: 0,
            position_ms: self.position_ms(),
            paused: self.paused(),
            device: self.services.remote_device.clone(),
        })
    }

    pub(in crate::app) fn restore_service_session(&mut self, saved: ServiceSession) -> Result<()> {
        let connections = crate::services::list(&self.data_directory())?;
        let connection = connections
            .iter()
            .find(|connection| connection.id == saved.connection && connection.enabled)
            .cloned()
            .context(self.text(
                "The previous music service connection is no longer available",
                "Прежнее подключение к музыкальному сервису больше не доступно",
            ))?;
        let remote = connection.provider == Provider::Spotify;
        let track = saved.queue.get(saved.index).cloned();
        if !remote {
            track.as_ref().context(self.text(
                "The last played service track is unavailable",
                "Последняя композиция из сервиса недоступна",
            ))?;
        }
        self.services.connection = Some(connection.id.clone());
        self.services.connections = connections;
        self.request_service_play(PlayRequest {
            connection,
            track,
            queue: saved.queue,
            index: saved.index,
            position: saved.position_ms,
            paused: saved.paused,
            // Spotify runs on its own device even while Almavorn is closed.
            // Reattach to its current state instead of rewinding that device.
            remote: remote.then(|| "state".into()),
            device: saved.device,
            activate: true,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        Ok(())
    }
}
