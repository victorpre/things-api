use std::{fmt, future::Future, pin::Pin};

use tokio::process::Command;

pub trait ThingsTaskCreator: Send + Sync {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsAddError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct ThingsUrlClient;

impl ThingsUrlClient {
    pub fn new() -> Self {
        Self
    }

    async fn create_task(&self, title: String) -> Result<(), ThingsAddError> {
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

        Ok(())
    }
}

impl ThingsTaskCreator for ThingsUrlClient {
    fn create_inbox_task<'a>(
        &'a self,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsAddError>> + Send + 'a>> {
        Box::pin(async move { self.create_task(title).await })
    }
}

#[derive(Debug)]
pub enum ThingsAddError {
    Launch(std::io::Error),
    LaunchStatus(Option<i32>),
}

impl fmt::Display for ThingsAddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launch(error) => write!(f, "failed to launch Things URL: {error}"),
            Self::LaunchStatus(code) => {
                write!(f, "Things URL launcher exited with status {code:?}")
            }
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
    ) -> Pin<Box<dyn Future<Output = Result<(), ThingsAddError>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
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
