use super::*;
use sqlx::{
    Executor,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[tokio::test]
async fn lists_tasks_with_related_metadata() {
    let fixture = FixtureDb::new().await;
    let repository = ThingsRepository::new(fixture.read_pool().await);

    let tasks = repository.list_tasks(TaskFilter::default()).await.unwrap();

    assert_eq!(tasks.len(), 2);

    let inbox = tasks.iter().find(|task| task.uuid == "task-open").unwrap();
    assert_eq!(inbox.title.as_deref(), Some("Open task"));
    assert_eq!(inbox.status, TaskStatus::Open);
    assert!(!inbox.trashed);
    assert_eq!(inbox.project_title.as_deref(), Some("Project A"));
    assert_eq!(inbox.area_title.as_deref(), Some("Area A"));
    assert_eq!(inbox.heading_title.as_deref(), Some("Heading A"));
    assert_eq!(inbox.tags, vec!["Home", "Next"]);
    assert_eq!(inbox.checklist_items_count, 2);
    assert_eq!(inbox.open_checklist_items_count, 1);
    assert_eq!(inbox.start, Some(1));
    assert_eq!(inbox.start_date, Some(12345));
    assert_eq!(inbox.reminder_time, Some(2051));
    assert_eq!(inbox.deadline, Some(23456));

    let completed = tasks
        .iter()
        .find(|task| task.uuid == "task-completed")
        .unwrap();
    assert_eq!(completed.status, TaskStatus::Completed);
    assert_eq!(completed.status_code, 2);
}

#[tokio::test]
async fn excludes_trashed_tasks_by_default() {
    let fixture = FixtureDb::new().await;
    let repository = ThingsRepository::new(fixture.read_pool().await);

    let tasks = repository.list_tasks(TaskFilter::default()).await.unwrap();

    assert!(tasks.iter().all(|task| task.uuid != "task-trashed"));
}

#[tokio::test]
async fn includes_trashed_tasks_when_requested() {
    let fixture = FixtureDb::new().await;
    let repository = ThingsRepository::new(fixture.read_pool().await);

    let tasks = repository
        .list_tasks(TaskFilter {
            include_trashed: true,
            ..TaskFilter::default()
        })
        .await
        .unwrap();

    assert!(tasks.iter().any(|task| task.uuid == "task-trashed"));
}

#[tokio::test]
async fn filters_by_status_and_trashed_flag() {
    let fixture = FixtureDb::new().await;
    let repository = ThingsRepository::new(fixture.read_pool().await);

    let completed = repository
        .list_tasks(TaskFilter {
            status: Some(2),
            trashed: None,
            include_trashed: false,
        })
        .await
        .unwrap();
    let trashed = repository
        .list_tasks(TaskFilter {
            status: None,
            trashed: Some(true),
            include_trashed: false,
        })
        .await
        .unwrap();

    assert_eq!(completed.len(), 1);
    assert_eq!(completed[0].uuid, "task-completed");
    assert_eq!(trashed.len(), 1);
    assert_eq!(trashed[0].uuid, "task-trashed");
}

struct FixtureDb {
    path: PathBuf,
}

impl FixtureDb {
    async fn new() -> Self {
        let path = unique_temp_path("things-fixture.sqlite");
        let _ = std::fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();

        create_schema(&pool).await;
        seed_data(&pool).await;
        pool.close().await;

        Self { path }
    }

    async fn read_pool(&self) -> SqlitePool {
        let options = SqliteConnectOptions::new()
            .filename(&self.path)
            .read_only(true)
            .create_if_missing(false);

        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap()
    }
}

impl Drop for FixtureDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn create_schema(pool: &SqlitePool) {
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

        CREATE TABLE TMArea (
            uuid TEXT PRIMARY KEY,
            title TEXT
        );

        CREATE TABLE TMChecklistItem (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            status INTEGER,
            "index" INTEGER,
            task TEXT
        );

        CREATE TABLE TMTaskTag (
            tasks TEXT NOT NULL,
            tags TEXT NOT NULL
        );

        CREATE TABLE TMTag (
            uuid TEXT PRIMARY KEY,
            title TEXT,
            "index" INTEGER
        );
        "#,
    )
    .await
    .unwrap();
}

async fn seed_data(pool: &SqlitePool) {
    pool.execute(
        r#"
        INSERT INTO TMArea (uuid, title) VALUES ('area-a', 'Area A');
        INSERT INTO TMTask (uuid, type, status, trashed, title, area)
            VALUES ('project-a', 1, 0, 0, 'Project A', 'area-a');
        INSERT INTO TMTask (uuid, type, status, trashed, title, project)
            VALUES ('heading-a', 2, 0, 0, 'Heading A', 'project-a');

        INSERT INTO TMTask (
            uuid, creationDate, userModificationDate, type, status, trashed,
            title, notes, start, startDate, reminderTime, deadline, "index",
            todayIndex, area, project, heading
        )
        VALUES (
            'task-open', 1.5, 2.5, 0, 0, 0, 'Open task', 'Notes', 1, 12345,
            2051, 23456, 2, 1, 'area-a', 'project-a', 'heading-a'
        );
        INSERT INTO TMTask (uuid, type, status, trashed, title, "index")
            VALUES ('task-completed', 0, 2, 0, 'Completed task', 3);
        INSERT INTO TMTask (uuid, type, status, trashed, title, "index")
            VALUES ('task-trashed', 0, 0, 1, 'Trashed task', 4);

        INSERT INTO TMTag (uuid, title, "index")
            VALUES ('tag-home', 'Home', 1), ('tag-next', 'Next', 2);
        INSERT INTO TMTaskTag (tasks, tags)
            VALUES ('task-open', 'tag-home'), ('task-open', 'tag-next');

        INSERT INTO TMChecklistItem (uuid, title, status, "index", task)
            VALUES
                ('check-open', 'Open check', 0, 1, 'task-open'),
                ('check-done', 'Done check', 2, 2, 'task-open');
        "#,
    )
    .await
    .unwrap();
}

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    std::env::temp_dir().join(format!(
        "things-api-repo-test-{}-{nanos}-{}-{name}",
        std::process::id(),
        TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}
