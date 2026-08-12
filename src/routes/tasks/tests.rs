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
    assert_eq!(parse_status("completed").unwrap(), 2);
    assert_eq!(parse_status("canceled").unwrap(), 3);
    assert_eq!(parse_status("cancelled").unwrap(), 3);
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

    assert_eq!(response.tasks.len(), 1);
    assert_eq!(response.tasks[0].uuid, "route-task");
    assert_eq!(response.tasks[0].status_code, 0);
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
            "index" INTEGER,
            todayIndex INTEGER,
            area TEXT,
            project TEXT,
            heading TEXT
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
                ('route-completed', 0, 2, 0, 'Completed route task', 2);
        "#,
    )
    .await
    .unwrap();

    pool
}
