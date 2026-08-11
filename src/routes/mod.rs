use axum::Router;

mod health;

pub fn router() -> Router {
    Router::new().merge(health::router())
}
