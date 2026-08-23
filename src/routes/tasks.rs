use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    audio::{AudioUpload, WhisperError, normalize_transcript},
    things::{TaskFilter, TaskList, ThingsTask},
    things_url::ThingsWriteError,
};

const AUDIO_UPLOAD_LIMIT_BYTES: usize = 50 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", patch(update_task))
        .route("/tasks/lists", get(list_task_lists))
        .route("/tasks/today", get(list_today_tasks))
        .route(
            "/tasks/from-audio",
            post(create_task_from_audio).layer(DefaultBodyLimit::max(AUDIO_UPLOAD_LIMIT_BYTES)),
        )
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let filter = query.into_filter()?;
    let tasks = state.repository.list_tasks(filter).await?;

    Ok(Json(TasksResponse { tasks }))
}

async fn list_task_lists(
    State(state): State<AppState>,
    Query(query): Query<ListTaskListsQuery>,
) -> Result<Json<TaskListsResponse>, ApiError> {
    let selected_lists = query.into_selected_lists()?;
    let mut lists = Vec::with_capacity(selected_lists.len());

    for list in selected_lists {
        let tasks = state.repository.list_task_list(list).await?;
        lists.push(TaskListResponse {
            id: task_list_id(list),
            title: task_list_title(list),
            tasks,
        });
    }

    Ok(Json(TaskListsResponse { lists }))
}

async fn list_today_tasks(
    State(state): State<AppState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<TasksResponse>, ApiError> {
    let filter = query.into_filter_for_today()?;
    let tasks = state.repository.list_tasks(filter).await?;

    Ok(Json(TasksResponse { tasks }))
}

async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateTaskRequest>,
) -> Result<(StatusCode, Json<UpdateTaskResponse>), ApiError> {
    let attributes = payload.into_attributes()?;

    state
        .things_task_writer
        .update_task_status(id.clone(), attributes.completed, attributes.canceled)
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(UpdateTaskResponse { id, attributes }),
    ))
}

async fn create_task_from_audio(
    State(state): State<AppState>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<CreateTaskResponse>), ApiError> {
    let upload = extract_audio_upload(multipart).await?;
    let transcript = state.whisper_transcriber.transcribe(upload).await?;
    let title = normalize_transcript(&transcript);
    if title.is_empty() {
        return Err(ApiError::unprocessable_entity("transcript was empty"));
    }

    state
        .things_task_writer
        .create_inbox_task(title.clone())
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(CreateTaskResponse {
            attributes: CreatedTaskAttributes { title },
        }),
    ))
}

async fn extract_audio_upload(mut multipart: Multipart) -> Result<AudioUpload, ApiError> {
    while let Some(field) = multipart.next_field().await.map_err(ApiError::multipart)? {
        if field.name() != Some("file") {
            continue;
        }

        let filename = field.file_name().map(str::to_string);
        let content_type = field.content_type().map(str::to_string);
        let bytes = field.bytes().await.map_err(ApiError::multipart)?;
        if bytes.is_empty() {
            return Err(ApiError::bad_request(
                "multipart field file must not be empty",
            ));
        }

        return Ok(AudioUpload {
            bytes: bytes.to_vec(),
            filename,
            content_type,
        });
    }

    Err(ApiError::bad_request("multipart field file is required"))
}

#[derive(Debug, Deserialize, Default)]
struct ListTasksQuery {
    status: Option<String>,
    trashed: Option<bool>,
    include_trashed: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct ListTaskListsQuery {
    selected: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskRequest {
    completed: Option<bool>,
    canceled: Option<bool>,
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
            today_only: false,
        })
    }

    fn into_filter_for_today(self) -> Result<TaskFilter, ApiError> {
        let mut filter = self.into_filter()?;
        filter.status = None;
        filter.trashed = None;
        filter.include_trashed = false;
        filter.today_only = true;

        Ok(filter)
    }
}

impl ListTaskListsQuery {
    fn into_selected_lists(self) -> Result<Vec<TaskList>, ApiError> {
        let Some(selected) = self.selected else {
            return Ok(vec![TaskList::Inbox, TaskList::Today]);
        };

        if selected.is_empty() {
            return Err(ApiError::bad_request(
                "selected must include inbox, today, or both",
            ));
        }

        let mut lists = Vec::new();
        for value in selected.split(',') {
            let value = value.trim();
            if value.is_empty() {
                return Err(ApiError::bad_request(
                    "selected must not include empty list ids",
                ));
            }

            let list = parse_task_list(value)?;
            if lists.contains(&list) {
                return Err(ApiError::bad_request(
                    "selected must not include duplicate list ids",
                ));
            }
            lists.push(list);
        }

        Ok(lists)
    }
}

impl UpdateTaskRequest {
    fn into_attributes(self) -> Result<UpdateTaskAttributes, ApiError> {
        if self.completed.is_none() && self.canceled.is_none() {
            return Err(ApiError::bad_request(
                "at least one of completed or canceled is required",
            ));
        }

        Ok(UpdateTaskAttributes {
            completed: self.completed,
            canceled: self.canceled,
        })
    }
}

fn parse_status(status: &str) -> Result<i64, ApiError> {
    match status {
        "open" => Ok(0),
        "completed" => Ok(3),
        "canceled" | "cancelled" => Ok(2),
        _ => status.parse::<i64>().map_err(|_| {
            ApiError::bad_request(
                "status must be one of open, completed, canceled, or a raw Things status code",
            )
        }),
    }
}

fn parse_task_list(value: &str) -> Result<TaskList, ApiError> {
    match value {
        "inbox" => Ok(TaskList::Inbox),
        "today" => Ok(TaskList::Today),
        _ => Err(ApiError::bad_request(
            "selected must contain only inbox and today",
        )),
    }
}

fn task_list_id(list: TaskList) -> &'static str {
    match list {
        TaskList::Inbox => "inbox",
        TaskList::Today => "today",
    }
}

fn task_list_title(list: TaskList) -> &'static str {
    match list {
        TaskList::Inbox => "Inbox",
        TaskList::Today => "Today",
    }
}

#[derive(Debug, Serialize)]
struct TasksResponse {
    tasks: Vec<ThingsTask>,
}

#[derive(Debug, Serialize)]
struct TaskListsResponse {
    lists: Vec<TaskListResponse>,
}

#[derive(Debug, Serialize)]
struct TaskListResponse {
    id: &'static str,
    title: &'static str,
    tasks: Vec<ThingsTask>,
}

#[derive(Debug, Serialize)]
struct CreateTaskResponse {
    attributes: CreatedTaskAttributes,
}

#[derive(Debug, Serialize)]
struct CreatedTaskAttributes {
    title: String,
}

#[derive(Debug, Serialize)]
struct UpdateTaskResponse {
    id: String,
    attributes: UpdateTaskAttributes,
}

#[derive(Debug, Serialize)]
struct UpdateTaskAttributes {
    completed: Option<bool>,
    canceled: Option<bool>,
}

#[derive(Debug)]
enum ApiError {
    BadRequest { message: &'static str },
    Database(sqlx::Error),
    Multipart(axum::extract::multipart::MultipartError),
    ThingsWrite(ThingsWriteError),
    UnprocessableEntity { message: &'static str },
    Whisper(WhisperError),
}

impl ApiError {
    fn bad_request(message: &'static str) -> Self {
        Self::BadRequest { message }
    }

    fn multipart(error: axum::extract::multipart::MultipartError) -> Self {
        Self::Multipart(error)
    }

    fn unprocessable_entity(message: &'static str) -> Self {
        Self::UnprocessableEntity { message }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<WhisperError> for ApiError {
    fn from(error: WhisperError) -> Self {
        Self::Whisper(error)
    }
}

impl From<ThingsWriteError> for ApiError {
    fn from(error: ThingsWriteError) -> Self {
        Self::ThingsWrite(error)
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
            Self::Multipart(error) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "invalid multipart request",
                }),
            )
                .into_response_with_log(error),
            Self::UnprocessableEntity { message } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),
            Self::Whisper(error) => {
                let response = (
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse {
                        error: match error {
                            WhisperError::InvalidResponse(_) => {
                                "failed to parse Whisper transcript response"
                            }
                            _ => "failed to transcribe audio with Whisper",
                        },
                    }),
                );
                response.into_response_with_log(error)
            }
            Self::ThingsWrite(error) => match error {
                ThingsWriteError::MissingUpdateAuthToken => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Things update auth token is not configured",
                    }),
                )
                    .into_response_with_log(error),
                ThingsWriteError::Launch { operation, .. }
                | ThingsWriteError::LaunchStatus { operation, .. } => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: match operation.action_name() {
                            "add" => "failed to open Things add URL",
                            "update" => "failed to open Things update URL",
                            _ => "failed to open Things URL",
                        },
                    }),
                )
                    .into_response_with_log(error),
            },
        }
    }
}

trait IntoResponseWithLog {
    fn into_response_with_log<E>(self, error: E) -> Response
    where
        E: std::fmt::Debug;
}

impl<T> IntoResponseWithLog for T
where
    T: IntoResponse,
{
    fn into_response_with_log<E>(self, error: E) -> Response
    where
        E: std::fmt::Debug,
    {
        eprintln!("request failed: {error:?}");
        self.into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

#[cfg(test)]
mod tests;
