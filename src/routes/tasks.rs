use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    things::{TaskFilter, ThingsTask},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/tasks", get(list_tasks))
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let filter = query.into_filter()?;
    let tasks = state.repository.list_tasks(filter).await?;

    Ok(Json(TasksResponse { tasks }))
}

#[derive(Debug, Deserialize, Default)]
struct ListTasksQuery {
    status: Option<String>,
    trashed: Option<bool>,
    include_trashed: Option<bool>,
}

impl ListTasksQuery {
    fn into_filter(self) -> Result<TaskFilter, ApiError> {
        Ok(TaskFilter {
            status: self
                .status
                .map(|status| parse_status(&status))
                .transpose()?,
            trashed: self.trashed,
            include_trashed: self.include_trashed.unwrap_or(false),
        })
    }
}

fn parse_status(status: &str) -> Result<i64, ApiError> {
    match status {
        "open" => Ok(0),
        "completed" => Ok(2),
        "canceled" | "cancelled" => Ok(3),
        _ => status.parse::<i64>().map_err(|_| {
            ApiError::bad_request(
                "status must be one of open, completed, canceled, or a raw Things status code",
            )
        }),
    }
}

#[derive(Debug, Serialize)]
struct TasksResponse {
    tasks: Vec<ThingsTask>,
}

#[derive(Debug)]
enum ApiError {
    BadRequest { message: &'static str },
    Database(sqlx::Error),
}

impl ApiError {
    fn bad_request(message: &'static str) -> Self {
        Self::BadRequest { message }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::BadRequest { message } => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),
            Self::Database(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "failed to read Things database",
                }),
            )
                .into_response_with_log(error),
        }
    }
}

trait IntoResponseWithLog {
    fn into_response_with_log(self, error: sqlx::Error) -> Response;
}

impl<T> IntoResponseWithLog for T
where
    T: IntoResponse,
{
    fn into_response_with_log(self, error: sqlx::Error) -> Response {
        eprintln!("failed to read Things database: {error}");
        self.into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

#[cfg(test)]
mod tests;
