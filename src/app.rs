use axum::Router;

use crate::{routes, things::ThingsRepository};

#[derive(Clone)]
pub struct AppState {
    pub repository: ThingsRepository,
}

impl AppState {
    pub fn new(repository: ThingsRepository) -> Self {
        Self { repository }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new().merge(routes::router()).with_state(state)
}
