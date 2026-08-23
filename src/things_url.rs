use std::{fmt, future::Future, pin::Pin};

use tokio::process::Command;

pub trait ThingsTaskWriter: Send + Sync {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>>;

    fn update_task_status<'a>(
        &'a self,
        id: String,
        completed: Option<bool>,
        canceled: Option<bool>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct ThingsUrlClient {
    auth_token: Option<String>,
}

impl ThingsUrlClient {
    pub fn new(auth_token: Option<String>) -> Self {
        Self { auth_token }
    }

    async fn create_task(&self, title: String) -> Result<(), ThingsWriteError> {
        let url = build_add_task_url(&title);
        launch_url(ThingsUrlOperation::Add, &url).await
    }

    async fn update_status(
        &self,
        id: String,
        completed: Option<bool>,
        canceled: Option<bool>,
    ) -> Result<(), ThingsWriteError> {
        let auth_token = self
            .auth_token
            .as_deref()
            .ok_or(ThingsWriteError::MissingUpdateAuthToken)?;
        let url = build_update_task_status_url(auth_token, &id, completed, canceled);
        launch_url(ThingsUrlOperation::Update, &url).await
    }
}

impl ThingsTaskWriter for ThingsUrlClient {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>> {
        Box::pin(async move { self.create_task(title).await })
    }

    fn update_task_status<'a>(
        &'a self,
        id: String,
        completed: Option<bool>,
        canceled: Option<bool>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>> {
        Box::pin(async move { self.update_status(id, completed, canceled).await })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThingsUrlOperation {
    Add,
    Update,
}

impl ThingsUrlOperation {
    pub fn action_name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Update => "update",
        }
    }
}

#[derive(Debug)]
pub enum ThingsWriteError {
    MissingUpdateAuthToken,
    Launch {
        operation: ThingsUrlOperation,
        error: std::io::Error,
    },
    LaunchStatus {
        operation: ThingsUrlOperation,
        code: Option<i32>,
    },
}

impl fmt::Display for ThingsWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingUpdateAuthToken => {
                write!(f, "Things update auth token is not configured")
            }
            Self::Launch { error, .. } => write!(f, "failed to launch Things URL: {error}"),
            Self::LaunchStatus { code, .. } => {
                write!(f, "Things URL launcher exited with status {code:?}")
            }
        }
    }
}

impl std::error::Error for ThingsWriteError {}

#[cfg(test)]
#[derive(Debug)]
pub struct DisabledThingsTaskWriter;

#[cfg(test)]
impl ThingsTaskWriter for DisabledThingsTaskWriter {
    fn create_inbox_task<'a>(
        &'a self,
        _title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }

    fn update_task_status<'a>(
        &'a self,
        _id: String,
        _completed: Option<bool>,
        _canceled: Option<bool>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsWriteError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

pub fn build_add_task_url(title: &str) -> String {
    format!("things:///add?title={}", encode_query_value(title))
}

pub fn build_update_task_status_url(
    auth_token: &str,
    id: &str,
    completed: Option<bool>,
    canceled: Option<bool>,
) -> String {
    let mut url = format!(
        "things:///update?auth-token={}&id={}",
        encode_query_value(auth_token),
        encode_query_value(id)
    );

    if let Some(completed) = completed {
        url.push_str("&completed=");
        url.push_str(bool_to_query_value(completed));
    }

    if let Some(canceled) = canceled {
        url.push_str("&canceled=");
        url.push_str(bool_to_query_value(canceled));
    }

    url
}

async fn launch_url(operation: ThingsUrlOperation, url: &str) -> Result<(), ThingsWriteError> {
    let status = Command::new("open")
        .arg("-g")
        .arg("-j")
        .arg(url)
        .status()
        .await
        .map_err(|error| ThingsWriteError::Launch { operation, error })?;

    if !status.success() {
        return Err(ThingsWriteError::LaunchStatus {
            operation,
            code: status.code(),
        });
    }

    Ok(())
}

fn bool_to_query_value(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_task_url_uses_add_command_without_callbacks() {
        let url = build_add_task_url("Clean coffee machine");

        assert_eq!(url, "things:///add?title=Clean%20coffee%20machine");
        assert!(!url.contains("x-success"));
        assert!(!url.contains("x-error"));
        assert!(!url.contains("x-cancel"));
        assert!(!url.contains("when="));
        assert!(!url.contains("list="));
        assert!(!url.contains("list-id="));
    }

    #[test]
    fn update_task_status_url_includes_auth_token_id_and_requested_fields() {
        let url = build_update_task_status_url("secret token", "task id", Some(true), Some(false));

        assert_eq!(
            url,
            "things:///update?auth-token=secret%20token&id=task%20id&completed=true&canceled=false"
        );
    }

    #[test]
    fn update_task_status_url_omits_absent_fields() {
        let url = build_update_task_status_url("secret", "task-id", None, Some(true));

        assert_eq!(
            url,
            "things:///update?auth-token=secret&id=task-id&canceled=true"
        );
        assert!(!url.contains("completed="));
    }

    #[test]
    fn query_encoding_uses_percent_twenty_for_spaces() {
        assert_eq!(
            encode_query_value("Clean coffee machine."),
            "Clean%20coffee%20machine."
        );
        assert_eq!(encode_query_value("A+B"), "A%2BB");
    }
}
