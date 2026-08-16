use std::collections::HashMap;

use serde::Serialize;
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};

#[derive(Clone)]
pub struct ThingsRepository {
    pool: SqlitePool,
}

impl ThingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<ThingsTask>, sqlx::Error> {
        let rows = self.fetch_task_rows(filter).await?;
        let mut tags_by_task = self.fetch_tags_by_task(&rows).await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let tags = tags_by_task.remove(&row.uuid).unwrap_or_default();
                row.into_task(tags)
            })
            .collect())
    }

    pub async fn find_inbox_task_id_created_after(
        &self,
        title: &str,
        created_after: f64,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            r#"
            SELECT uuid
            FROM TMTask
            WHERE type = 0
                AND status = 0
                AND trashed = 0
                AND title = ?
                AND creationDate >= ?
                AND area IS NULL
                AND project IS NULL
                AND heading IS NULL
            ORDER BY creationDate DESC, userModificationDate DESC
            LIMIT 1
            "#,
        )
        .bind(title)
        .bind(created_after)
        .fetch_optional(&self.pool)
        .await
    }

    async fn fetch_task_rows(&self, filter: TaskFilter) -> Result<Vec<TaskRow>, sqlx::Error> {
        let mut query = QueryBuilder::<Sqlite>::new(if filter.today_only {
            r#"
            WITH today(code) AS (
                SELECT
                    (CAST(strftime('%Y', 'now', 'localtime') AS INTEGER) << 16)
                    | (CAST(strftime('%m', 'now', 'localtime') AS INTEGER) << 12)
                    | (CAST(strftime('%d', 'now', 'localtime') AS INTEGER) << 7)
            )
            SELECT
                task.uuid,
                task.title,
                task.notes,
                task.status,
                task.trashed,
                task.creationDate AS creation_date,
                task.userModificationDate AS user_modification_date,
                task.start,
                task.startDate AS start_date,
                task.reminderTime AS reminder_time,
                task.deadline,
                COALESCE(task.project, heading.project) AS project_id,
                COALESCE(project.title, heading_project.title) AS project_title,
                task.area AS area_id,
                area.title AS area_title,
                task.heading AS heading_id,
                heading.title AS heading_title,
                COALESCE(checklist.checklist_items_count, 0) AS checklist_items_count,
                COALESCE(checklist.open_checklist_items_count, 0) AS open_checklist_items_count
            FROM TMTask AS task
            LEFT JOIN TMTask AS project
                ON task.project = project.uuid AND project.type = 1
            LEFT JOIN TMArea AS area
                ON task.area = area.uuid
            LEFT JOIN TMTask AS heading
                ON task.heading = heading.uuid AND heading.type = 2
            LEFT JOIN TMTask AS heading_project
                ON heading.project = heading_project.uuid AND heading_project.type = 1
            LEFT JOIN (
                SELECT
                    task,
                    COUNT(*) AS checklist_items_count,
                    SUM(CASE WHEN status = 0 THEN 1 ELSE 0 END) AS open_checklist_items_count
                FROM TMChecklistItem
                GROUP BY task
            ) AS checklist
                ON checklist.task = task.uuid
            CROSS JOIN today
            "#
        } else {
            r#"
            SELECT
                task.uuid,
                task.title,
                task.notes,
                task.status,
                task.trashed,
                task.creationDate AS creation_date,
                task.userModificationDate AS user_modification_date,
                task.start,
                task.startDate AS start_date,
                task.reminderTime AS reminder_time,
                task.deadline,
                COALESCE(task.project, heading.project) AS project_id,
                COALESCE(project.title, heading_project.title) AS project_title,
                task.area AS area_id,
                area.title AS area_title,
                task.heading AS heading_id,
                heading.title AS heading_title,
                COALESCE(checklist.checklist_items_count, 0) AS checklist_items_count,
                COALESCE(checklist.open_checklist_items_count, 0) AS open_checklist_items_count
            FROM TMTask AS task
            LEFT JOIN TMTask AS project
                ON task.project = project.uuid AND project.type = 1
            LEFT JOIN TMArea AS area
                ON task.area = area.uuid
            LEFT JOIN TMTask AS heading
                ON task.heading = heading.uuid AND heading.type = 2
            LEFT JOIN TMTask AS heading_project
                ON heading.project = heading_project.uuid AND heading_project.type = 1
            LEFT JOIN (
                SELECT
                    task,
                    COUNT(*) AS checklist_items_count,
                    SUM(CASE WHEN status = 0 THEN 1 ELSE 0 END) AS open_checklist_items_count
                FROM TMChecklistItem
                GROUP BY task
            ) AS checklist
                ON checklist.task = task.uuid
            "#
        });

        query.push(
            r#"
            WHERE task.type = 0
            "#,
        );

        if filter.today_only {
            query.push(
                r#"
                AND task.status = 0
                AND task.trashed = 0
                AND task.rt1_recurrenceRule IS NULL
                AND COALESCE(project.trashed, heading_project.trashed, 0) = 0
                AND (
                    (task.start = 1 AND task.startDate IS NOT NULL AND task.startDate <= today.code)
                    OR (task.start = 2 AND task.startDate IS NOT NULL AND task.startDate <= today.code)
                    OR (
                        task.startDate IS NULL
                        AND task.deadline IS NOT NULL
                        AND task.deadline <= today.code
                        AND task.deadlineSuppressionDate IS NULL
                    )
                )
                "#,
            );
        } else {
            if let Some(status) = filter.status {
                query.push(" AND task.status = ");
                query.push_bind(status);
            }

            match filter.trashed {
                Some(trashed) => {
                    query.push(" AND task.trashed = ");
                    query.push_bind(i64::from(trashed));
                }
                None if !filter.include_trashed => {
                    query.push(" AND task.trashed = 0");
                }
                None => {}
            }
        }

        query.push(
            r#"
            ORDER BY
                task.todayIndex IS NULL,
                task.todayIndex,
                task."index",
                task.title
            "#,
        );

        query
            .build_query_as::<TaskRow>()
            .fetch_all(&self.pool)
            .await
    }

    async fn fetch_tags_by_task(
        &self,
        rows: &[TaskRow],
    ) -> Result<HashMap<String, Vec<String>>, sqlx::Error> {
        if rows.is_empty() {
            return Ok(HashMap::new());
        }

        let mut query = QueryBuilder::<Sqlite>::new(
            r#"
            SELECT task_tag.tasks AS task_id, tag.title
            FROM TMTaskTag AS task_tag
            INNER JOIN TMTag AS tag ON task_tag.tags = tag.uuid
            WHERE task_tag.tasks IN (
            "#,
        );

        let mut separated = query.separated(", ");
        for row in rows {
            separated.push_bind(&row.uuid);
        }
        separated.push_unseparated(r#") ORDER BY task_tag.tasks, tag."index", tag.title"#);

        let tag_rows = query
            .build_query_as::<TagRow>()
            .fetch_all(&self.pool)
            .await?;

        let mut tags_by_task: HashMap<String, Vec<String>> = HashMap::new();
        for row in tag_rows {
            if let Some(title) = row.title {
                tags_by_task.entry(row.task_id).or_default().push(title);
            }
        }

        Ok(tags_by_task)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskFilter {
    pub status: Option<i64>,
    pub trashed: Option<bool>,
    pub include_trashed: bool,
    pub today_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ThingsTask {
    pub uuid: String,
    pub title: Option<String>,
    pub notes: Option<String>,
    pub status_code: i64,
    pub status: TaskStatus,
    pub trashed: bool,
    pub creation_date: Option<f64>,
    pub user_modification_date: Option<f64>,
    pub start: Option<i64>,
    pub start_date: Option<i64>,
    pub reminder_time: Option<i64>,
    pub deadline: Option<i64>,
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub area_id: Option<String>,
    pub area_title: Option<String>,
    pub heading_id: Option<String>,
    pub heading_title: Option<String>,
    pub tags: Vec<String>,
    pub checklist_items_count: i64,
    pub open_checklist_items_count: i64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Completed,
    Canceled,
    Unknown,
}

impl TaskStatus {
    fn from_code(code: i64) -> Self {
        match code {
            0 => Self::Open,
            2 => Self::Canceled,
            3 => Self::Completed,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, FromRow)]
struct TaskRow {
    uuid: String,
    title: Option<String>,
    notes: Option<String>,
    status: i64,
    trashed: i64,
    creation_date: Option<f64>,
    user_modification_date: Option<f64>,
    start: Option<i64>,
    start_date: Option<i64>,
    reminder_time: Option<i64>,
    deadline: Option<i64>,
    project_id: Option<String>,
    project_title: Option<String>,
    area_id: Option<String>,
    area_title: Option<String>,
    heading_id: Option<String>,
    heading_title: Option<String>,
    checklist_items_count: i64,
    open_checklist_items_count: i64,
}

impl TaskRow {
    fn into_task(self, tags: Vec<String>) -> ThingsTask {
        ThingsTask {
            uuid: self.uuid,
            title: self.title,
            notes: self.notes,
            status_code: self.status,
            status: TaskStatus::from_code(self.status),
            trashed: self.trashed != 0,
            creation_date: self.creation_date,
            user_modification_date: self.user_modification_date,
            start: self.start,
            start_date: self.start_date,
            reminder_time: self.reminder_time,
            deadline: self.deadline,
            project_id: self.project_id,
            project_title: self.project_title,
            area_id: self.area_id,
            area_title: self.area_title,
            heading_id: self.heading_id,
            heading_title: self.heading_title,
            tags,
            checklist_items_count: self.checklist_items_count,
            open_checklist_items_count: self.open_checklist_items_count,
        }
    }
}

#[derive(Debug, FromRow)]
struct TagRow {
    task_id: String,
    title: Option<String>,
}

#[cfg(test)]
mod tests;
