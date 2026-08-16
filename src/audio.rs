use std::{fmt, future::Future, pin::Pin};

use serde::Deserialize;

const DEFAULT_FILE_NAME: &str = "audio.wav";
const DEFAULT_CONTENT_TYPE: &str = "audio/wav";
const WHISPER_FORM_FIELDS: &[(&str, &str)] = &[
    ("temperature", "0.0"),
    ("temperature_inc", "0.2"),
    ("response_format", "json"),
    ("prompt", "Identify the task todo"),
    ("carry_initial_prompt", "true"),
];

#[derive(Debug)]
pub struct AudioUpload {
    pub bytes: Vec<u8>,
    pub filename: Option<String>,
    pub content_type: Option<String>,
}

pub trait WhisperTranscriber: Send + Sync {
    fn transcribe<'a>(
        &'a self,
        upload: AudioUpload,
    ) -> Pin<Box<dyn Future<Output = Result<String, WhisperError>> + Send + 'a>>;
}

#[derive(Clone)]
pub struct WhisperClient {
    client: reqwest::Client,
    inference_url: String,
}

impl WhisperClient {
    pub fn new(inference_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            inference_url,
        }
    }

    async fn transcribe_upload(&self, upload: AudioUpload) -> Result<String, WhisperError> {
        let filename = upload
            .filename
            .filter(|filename| !filename.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_FILE_NAME.to_string());
        let content_type = upload
            .content_type
            .filter(|content_type| !content_type.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CONTENT_TYPE.to_string());

        let file = reqwest::multipart::Part::bytes(upload.bytes)
            .file_name(filename)
            .mime_str(&content_type)
            .map_err(WhisperError::InvalidMultipart)?;
        let mut form = reqwest::multipart::Form::new().part("file", file);
        for (name, value) in WHISPER_FORM_FIELDS {
            form = form.text(*name, *value);
        }

        let response = self
            .client
            .post(&self.inference_url)
            .multipart(form)
            .send()
            .await
            .map_err(WhisperError::Request)?;

        let status = response.status();
        if !status.is_success() {
            return Err(WhisperError::UpstreamStatus(status.as_u16()));
        }

        let body = response.bytes().await.map_err(WhisperError::Request)?;
        parse_whisper_text(&body)
    }
}

impl WhisperTranscriber for WhisperClient {
    fn transcribe<'a>(
        &'a self,
        upload: AudioUpload,
    ) -> Pin<Box<dyn Future<Output = Result<String, WhisperError>> + Send + 'a>> {
        Box::pin(async move { self.transcribe_upload(upload).await })
    }
}

#[derive(Debug)]
pub enum WhisperError {
    Request(reqwest::Error),
    InvalidMultipart(reqwest::Error),
    InvalidResponse(serde_json::Error),
    UpstreamStatus(u16),
}

impl fmt::Display for WhisperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(f, "Whisper request failed: {error}"),
            Self::InvalidMultipart(error) => {
                write!(f, "failed to build Whisper multipart request: {error}")
            }
            Self::InvalidResponse(error) => write!(f, "invalid Whisper JSON response: {error}"),
            Self::UpstreamStatus(status) => {
                write!(f, "Whisper returned non-success status {status}")
            }
        }
    }
}

impl std::error::Error for WhisperError {}

#[cfg(test)]
#[derive(Debug)]
pub struct DisabledWhisperTranscriber;

#[cfg(test)]
impl WhisperTranscriber for DisabledWhisperTranscriber {
    fn transcribe<'a>(
        &'a self,
        _upload: AudioUpload,
    ) -> Pin<Box<dyn Future<Output = Result<String, WhisperError>> + Send + 'a>> {
        Box::pin(async { Err(WhisperError::UpstreamStatus(503)) })
    }
}

pub fn normalize_transcript(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_whisper_text(bytes: &[u8]) -> Result<String, WhisperError> {
    let response: WhisperResponse =
        serde_json::from_slice(bytes).map_err(WhisperError::InvalidResponse)?;
    Ok(response.text)
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_transcript_whitespace() {
        assert_eq!(
            normalize_transcript(" Clean   coffee\nmachine\tplease "),
            "Clean coffee machine please"
        );
    }

    #[test]
    fn parses_whisper_text_response() {
        let text = parse_whisper_text(br#"{"text":" Clean coffee machine\n"}"#).unwrap();

        assert_eq!(text, " Clean coffee machine\n");
    }

    #[test]
    fn rejects_malformed_whisper_response() {
        let error = parse_whisper_text(br#"{"message":"missing text"}"#).unwrap_err();

        assert!(matches!(error, WhisperError::InvalidResponse(_)));
    }

    #[test]
    fn whisper_form_fields_include_task_prompt() {
        assert!(WHISPER_FORM_FIELDS.contains(&("temperature", "0.0")));
        assert!(WHISPER_FORM_FIELDS.contains(&("temperature_inc", "0.2")));
        assert!(WHISPER_FORM_FIELDS.contains(&("response_format", "json")));
        assert!(WHISPER_FORM_FIELDS.contains(&("prompt", "Identify the task todo")));
        assert!(WHISPER_FORM_FIELDS.contains(&("carry_initial_prompt", "true")));
    }
}
