use std::{
    fmt,
    future::Future,
    pin::Pin,
    time::{Duration, SystemTime},
};

use tokio::{process::Command, time};

use crate::things::ThingsRepository;

const APPLE_REFERENCE_UNIX_SECONDS: f64 = 978_307_200.0;
const CREATION_LOOKUP_CLOCK_SKEW_SECONDS: f64 = 5.0;
const CREATION_LOOKUP_POLL_INTERVAL: Duration = Duration::from_millis(250);

pub trait ThingsTaskCreator: Send + Sync {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ThingsAddError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct ThingsUrlClient {
    repository: ThingsRepository,
    creation_timeout: Duration,
}

impl ThingsUrlClient {
    pub fn new(repository: ThingsRepository, creation_timeout: Duration) -> Self {
        Self {
            repository,
            creation_timeout,
        }
    }

    async fn create_task(&self, title: String) -> Result<String, ThingsAddError> {
        let created_after = apple_reference_now() - CREATION_LOOKUP_CLOCK_SKEW_SECONDS;
        let url = build_add_task_url(&title);

        let status = Command::new("open")
            .arg("-g")
            .arg("-j")
            .arg(&url)
            .status()
            .await
            .map_err(ThingsAddError::Launch)?;

        if !status.success() {
            return Err(ThingsAddError::LaunchStatus(status.code()));
        }

        let deadline = time::Instant::now() + self.creation_timeout;
        loop {
            if let Some(id) = self
                .repository
                .find_inbox_task_id_created_after(&title, created_after)
                .await
                .map_err(ThingsAddError::Database)?
            {
                return Ok(id);
            }

            if time::Instant::now() >= deadline {
                return Err(ThingsAddError::Timeout);
            }

            time::sleep(CREATION_LOOKUP_POLL_INTERVAL).await;
        }
    }
}

impl ThingsTaskCreator for ThingsUrlClient {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ThingsAddError>> + Send + 'a>> {
        Box::pin(async move { self.create_task(title).await })
    }
}

#[derive(Debug)]
pub enum ThingsAddError {
    Database(sqlx::Error),
    Launch(std::io::Error),
    LaunchStatus(Option<i32>),
    Timeout,
}

impl fmt::Display for ThingsAddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(f, "failed to look up created Things task: {error}"),
            Self::Launch(error) => write!(f, "failed to launch Things URL: {error}"),
            Self::LaunchStatus(code) => {
                write!(f, "Things URL launcher exited with status {code:?}")
            }
            Self::Timeout => write!(f, "timed out waiting for Things task to appear"),
        }
    }
}

impl std::error::Error for ThingsAddError {}

#[cfg(test)]
#[derive(Debug)]
pub struct DisabledThingsTaskCreator;

#[cfg(test)]
impl ThingsTaskCreator for DisabledThingsTaskCreator {
    fn create_inbox_task<'a>(
        &'a self,
        _title: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, ThingsAddError>> + Send + 'a>> {
        Box::pin(async { Err(ThingsAddError::Timeout) })
    }
}

pub fn build_add_task_url(title: &str) -> String {
    format!("things:///add?title={}", encode_query_value(title))
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

fn apple_reference_now() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        - APPLE_REFERENCE_UNIX_SECONDS
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
    fn query_encoding_uses_percent_twenty_for_spaces() {
        assert_eq!(
            encode_query_value("Clean coffee machine."),
            "Clean%20coffee%20machine."
        );
        assert_eq!(encode_query_value("A+B"), "A%2BB");
    }
}
