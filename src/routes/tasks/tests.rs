use super::*;
use crate::{
    audio::{AudioUpload, WhisperError, WhisperTranscriber},
    things::ThingsRepository,
    things_url::{ThingsAddError, ThingsTaskCreator},
};
use axum::{
    body::{Body, to_bytes},
    extract::Query,
    http::{Request, StatusCode, header},
};
use sqlx::{Executor, SqlitePool};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tower::ServiceExt;

#[test]
fn parses_named_status_filters() {
    assert_eq!(
        ListTasksQuery {
            status: Some("open".to_string()),
            ..ListTasksQuery::default()
        }
        .into_filter()
        .unwrap()
        .status,
        Some(0)
    );
    assert_eq!(parse_status("completed").unwrap(), 3);
    assert_eq!(parse_status("canceled").unwrap(), 2);
    assert_eq!(parse_status("cancelled").unwrap(), 2);
}

#[test]
fn parses_raw_status_filters() {
    assert_eq!(parse_status("9").unwrap(), 9);
}

#[test]
fn rejects_invalid_status_filters() {
    let error = parse_status("done").unwrap_err();

    assert!(matches!(error, ApiError::BadRequest { .. }));
}

#[test]
fn today_filter_uses_dedicated_today_query() {
    assert_eq!(
        ListTasksQuery::default()
            .into_filter_for_today()
            .unwrap()
            .status,
        None
    );
}

#[test]
fn today_filter_ignores_explicit_list_filters() {
    let filter = ListTasksQuery {
        status: Some("completed".to_string()),
        trashed: Some(true),
        include_trashed: Some(true),
    }
    .into_filter_for_today()
    .unwrap();

    assert_eq!(
        filter,
        TaskFilter {
            status: None,
            trashed: None,
            include_trashed: false,
            today_only: true,
        }
    );
}

#[test]
fn task_lists_query_defaults_to_inbox_and_today() {
    assert_eq!(
        ListTaskListsQuery::default().into_selected_lists().unwrap(),
        vec![TaskList::Inbox, TaskList::Today]
    );
}

#[test]
fn task_lists_query_preserves_selected_order() {
    assert_eq!(
        ListTaskListsQuery {
            selected: Some("today,inbox".to_string()),
        }
        .into_selected_lists()
        .unwrap(),
        vec![TaskList::Today, TaskList::Inbox]
    );
}

#[test]
fn task_lists_query_rejects_empty_unknown_and_duplicate_ids() {
    for selected in ["", "today,,inbox", "foo", "today,today"] {
        let error = ListTaskListsQuery {
            selected: Some(selected.to_string()),
        }
        .into_selected_lists()
        .unwrap_err();

        assert!(matches!(error, ApiError::BadRequest { .. }));
    }
}

#[tokio::test]
async fn list_tasks_handler_returns_tasks_from_repository() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_tasks(
        State(state),
        Query(ListTasksQuery {
            status: Some("open".to_string()),
            ..ListTasksQuery::default()
        }),
    )
    .await
    .unwrap();

    let ids = task_ids(&response.tasks);
    assert_eq!(
        ids,
        vec!["route-today-index-only", "route-task", "route-today-task"]
    );
    assert!(response.tasks.iter().all(|task| task.status_code == 0));
}

#[tokio::test]
async fn list_task_lists_handler_returns_inbox_and_today_by_default() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_task_lists(State(state), Query(ListTaskListsQuery::default()))
        .await
        .unwrap();

    assert_eq!(response.lists.len(), 2);
    assert_eq!(response.lists[0].id, "inbox");
    assert_eq!(response.lists[0].title, "Inbox");
    assert_eq!(task_ids(&response.lists[0].tasks), vec!["route-task"]);
    assert_eq!(response.lists[1].id, "today");
    assert_eq!(response.lists[1].title, "Today");
    assert_eq!(task_ids(&response.lists[1].tasks), vec!["route-today-task"]);
}

#[tokio::test]
async fn list_task_lists_handler_uses_selected_order() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_task_lists(
        State(state),
        Query(ListTaskListsQuery {
            selected: Some("today,inbox".to_string()),
        }),
    )
    .await
    .unwrap();

    assert_eq!(
        response
            .lists
            .iter()
            .map(|list| list.id)
            .collect::<Vec<_>>(),
        vec!["today", "inbox"]
    );
}

#[tokio::test]
async fn list_task_lists_handler_returns_selected_list_only() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_task_lists(
        State(state),
        Query(ListTaskListsQuery {
            selected: Some("inbox".to_string()),
        }),
    )
    .await
    .unwrap();

    assert_eq!(response.lists.len(), 1);
    assert_eq!(response.lists[0].id, "inbox");
}

#[tokio::test]
async fn list_task_lists_route_rejects_invalid_selected_values() {
    let state = AppState::new(ThingsRepository::new(fixture_pool().await));

    for selected in ["foo", "", "today,today"] {
        let response = crate::app::router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/tasks/lists?selected={selected}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn list_today_tasks_handler_returns_only_today_tasks() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_today_tasks(State(state), Query(ListTasksQuery::default()))
        .await
        .unwrap();

    assert_eq!(response.tasks.len(), 1);
    assert_eq!(response.tasks[0].uuid, "route-today-task");
}

#[tokio::test]
async fn create_task_from_audio_accepts_task_from_transcript() {
    let pool = fixture_pool().await;
    let things_titles = Arc::new(Mutex::new(Vec::new()));
    let state = AppState::with_write_services(
        ThingsRepository::new(pool),
        Arc::new(FakeWhisperTranscriber::text(" Clean coffee\nmachine ")),
        Arc::new(FakeThingsTaskCreator::success(Arc::clone(&things_titles))),
    );

    let response = crate::app::router(state)
        .oneshot(multipart_request(include_str!("tests.rs").as_bytes()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json.get("id").is_none());
    assert_eq!(json["attributes"]["title"], "Clean coffee machine");
    assert_eq!(
        things_titles.lock().unwrap().as_slice(),
        ["Clean coffee machine"]
    );
}

#[tokio::test]
async fn create_task_from_audio_rejects_missing_file() {
    let state = state_with_audio_fakes(
        FakeWhisperTranscriber::text("unused"),
        FakeThingsTaskCreator::success(Arc::new(Mutex::new(Vec::new()))),
    )
    .await;

    let request = Request::builder()
        .method("POST")
        .uri("/tasks/from-audio")
        .header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=BOUNDARY",
        )
        .body(Body::from("--BOUNDARY--\r\n"))
        .unwrap();
    let response = crate::app::router(state).oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_task_from_audio_rejects_empty_transcript() {
    let state = state_with_audio_fakes(
        FakeWhisperTranscriber::text(" \n\t "),
        FakeThingsTaskCreator::success(Arc::new(Mutex::new(Vec::new()))),
    )
    .await;

    let response = crate::app::router(state)
        .oneshot(multipart_request(b"wav"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn create_task_from_audio_maps_whisper_failure_to_bad_gateway() {
    let state = state_with_audio_fakes(
        FakeWhisperTranscriber::failure(),
        FakeThingsTaskCreator::success(Arc::new(Mutex::new(Vec::new()))),
    )
    .await;

    let response = crate::app::router(state)
        .oneshot(multipart_request(b"wav"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn create_task_from_audio_maps_things_launch_failure_to_server_error() {
    let state = state_with_audio_fakes(
        FakeWhisperTranscriber::text("Clean coffee machine"),
        FakeThingsTaskCreator::launch_failure(),
    )
    .await;

    let response = crate::app::router(state)
        .oneshot(multipart_request(b"wav"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

async fn fixture_pool() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    pool.execute(
        r#"
        CREATE TABLE TMTask (
            uuid TEXT PRIMARY KEY,
            creationDate REAL,
            userModificationDate REAL,
            type INTEGER,
            status INTEGER,
            trashed INTEGER,
            title TEXT,
            notes TEXT,
            start INTEGER,
            startDate INTEGER,
            reminderTime INTEGER,
            deadline INTEGER,
            deadlineSuppressionDate INTEGER,
            "index" INTEGER,
            todayIndex INTEGER,
            area TEXT,
            project TEXT,
            heading TEXT,
            rt1_recurrenceRule BLOB
        );
        CREATE TABLE TMArea (uuid TEXT PRIMARY KEY, title TEXT);
        CREATE TABLE TMChecklistItem (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            status INTEGER,
            "index" INTEGER,
            task TEXT
        );
        CREATE TABLE TMTaskTag (tasks TEXT NOT NULL, tags TEXT NOT NULL);
        CREATE TABLE TMTag (uuid TEXT PRIMARY KEY, title TEXT, "index" INTEGER);

        INSERT INTO TMTask (uuid, type, status, trashed, title, start, "index")
            VALUES
                ('route-task', 0, 0, 0, 'Route task', 0, 1),
                ('route-completed', 0, 3, 0, 'Completed route task', 0, 2);
        INSERT INTO TMTask (uuid, type, status, trashed, title, start, startDate, "index")
            VALUES ('route-today-task', 0, 0, 0, 'Today route task', 1, 1, 3);
        INSERT INTO TMTask (uuid, type, status, trashed, title, todayIndex, "index")
            VALUES ('route-today-index-only', 0, 0, 0, 'Today index only', 1, 4);
        "#,
    )
    .await
    .unwrap();

    pool
}

fn task_ids(tasks: &[ThingsTask]) -> Vec<&str> {
    tasks.iter().map(|task| task.uuid.as_str()).collect()
}

async fn state_with_audio_fakes(
    whisper: FakeWhisperTranscriber,
    things: FakeThingsTaskCreator,
) -> AppState {
    AppState::with_write_services(
        ThingsRepository::new(fixture_pool().await),
        Arc::new(whisper),
        Arc::new(things),
    )
}

fn multipart_request(file: &[u8]) -> Request<Body> {
    let mut body = Vec::new();
    body.extend_from_slice(
        b"--BOUNDARY\r\nContent-Disposition: form-data; name=\"file\"; filename=\"todo.wav\"\r\nContent-Type: audio/wav\r\n\r\n",
    );
    body.extend_from_slice(file);
    body.extend_from_slice(b"\r\n--BOUNDARY--\r\n");

    Request::builder()
        .method("POST")
        .uri("/tasks/from-audio")
        .header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=BOUNDARY",
        )
        .body(Body::from(body))
        .unwrap()
}

struct FakeWhisperTranscriber {
    response: FakeWhisperResponse,
}

impl FakeWhisperTranscriber {
    fn text(text: &'static str) -> Self {
        Self {
            response: FakeWhisperResponse::Text(text),
        }
    }

    fn failure() -> Self {
        Self {
            response: FakeWhisperResponse::Failure,
        }
    }
}

enum FakeWhisperResponse {
    Text(&'static str),
    Failure,
}

impl WhisperTranscriber for FakeWhisperTranscriber {
    fn transcribe<'a>(
        &'a self,
        upload: AudioUpload,
    ) -> Pin<Box<dyn Future<Output = Result<String, WhisperError>> + Send + 'a>> {
        Box::pin(async move {
            assert!(!upload.bytes.is_empty());
            match self.response {
                FakeWhisperResponse::Text(text) => Ok(text.to_string()),
                FakeWhisperResponse::Failure => Err(WhisperError::UpstreamStatus(500)),
            }
        })
    }
}

struct FakeThingsTaskCreator {
    response: FakeThingsResponse,
    titles: Arc<Mutex<Vec<String>>>,
}

impl FakeThingsTaskCreator {
    fn success(titles: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            response: FakeThingsResponse::Success,
            titles,
        }
    }

    fn launch_failure() -> Self {
        Self {
            response: FakeThingsResponse::LaunchFailure,
            titles: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

enum FakeThingsResponse {
    Success,
    LaunchFailure,
}

impl ThingsTaskCreator for FakeThingsTaskCreator {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsAddError>> + Send + 'a>> {
        Box::pin(async move {
            self.titles.lock().unwrap().push(title);
            match self.response {
                FakeThingsResponse::Success => Ok(()),
                FakeThingsResponse::LaunchFailure => Err(ThingsAddError::LaunchStatus(Some(1))),
            }
        })
    }
}
