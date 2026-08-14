use axum::Router;

use crate::app::AppState;

mod health;
mod tasks;

pub fn router() -> Router<AppState> {
    Router::new().merge(health::router()).merge(tasks::router())
}
