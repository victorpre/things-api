use super::*;
use crate::things::ThingsRepository;
use axum::extract::Query;
use sqlx::{Executor, SqlitePool};

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
async fn list_today_tasks_handler_returns_only_today_tasks() {
    let pool = fixture_pool().await;
    let state = AppState::new(ThingsRepository::new(pool));

    let Json(response) = list_today_tasks(State(state), Query(ListTasksQuery::default()))
        .await
        .unwrap();

    assert_eq!(response.tasks.len(), 1);
    assert_eq!(response.tasks[0].uuid, "route-today-task");
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

        INSERT INTO TMTask (uuid, type, status, trashed, title, "index")
            VALUES
                ('route-task', 0, 0, 0, 'Route task', 1),
                ('route-completed', 0, 3, 0, 'Completed route task', 2);
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
