use axum::Router;
use std::sync::Arc;

use crate::{
    audio::WhisperTranscriber, routes, things::ThingsRepository, things_url::ThingsTaskWriter,
};

#[cfg(test)]
use crate::{audio::DisabledWhisperTranscriber, things_url::DisabledThingsTaskWriter};

#[derive(Clone)]
pub struct AppState {
    pub repository: ThingsRepository,
    pub whisper_transcriber: Arc<dyn WhisperTranscriber>,
    pub things_task_writer: Arc<dyn ThingsTaskWriter>,
}

impl AppState {
    #[cfg(test)]
    pub fn new(repository: ThingsRepository) -> Self {
        Self::with_write_services(
            repository,
            Arc::new(DisabledWhisperTranscriber),
            Arc::new(DisabledThingsTaskWriter),
        )
    }

    pub fn with_write_services(
        repository: ThingsRepository,
        whisper_transcriber: Arc<dyn WhisperTranscriber>,
        things_task_writer: Arc<dyn ThingsTaskWriter>,
    ) -> Self {
        Self {
            repository,
            whisper_transcriber,
            things_task_writer,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new().merge(routes::router()).with_state(state)
}
