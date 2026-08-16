use std::{
    env,
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const DEFAULT_WHISPER_INFERENCE_URL: &str = "http://127.0.0.1:8080/inference";
const DEFAULT_THINGS_CREATION_TIMEOUT_SECS: u64 = 15;
const DEFAULT_THINGS_API_HOST: &str = "127.0.0.1";
const DEFAULT_THINGS_API_PORT: u16 = 3000;

#[derive(Debug, Clone)]
pub struct Config {
    pub things_db_path: PathBuf,
    pub whisper_inference_url: String,
    pub things_creation_timeout: Duration,
    pub things_api_host: String,
    pub things_api_port: u16,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let dotenv = DotenvValues::read();
        Self::from_raw(ConfigValues {
            things_db_path: read_config_os_value("THINGS_DB_PATH", &dotenv),
            whisper_inference_url: read_config_value("WHISPER_INFERENCE_URL", &dotenv),
            things_creation_timeout_secs: read_config_value(
                "THINGS_CREATION_TIMEOUT_SECS",
                &dotenv,
            ),
            things_api_host: read_config_value("THINGS_API_HOST", &dotenv),
            things_api_port: read_config_value("THINGS_API_PORT", &dotenv),
        })
    }

    #[cfg(test)]
    fn from_env_value(value: Option<OsString>) -> Result<Self, ConfigError> {
        Self::from_raw(ConfigValues {
            things_db_path: value,
            ..ConfigValues::default()
        })
    }

    fn from_raw(values: ConfigValues) -> Result<Self, ConfigError> {
        let value = values
            .things_db_path
            .ok_or(ConfigError::MissingThingsDbPath)?;

        if value.is_empty() {
            return Err(ConfigError::EmptyThingsDbPath);
        }

        let things_creation_timeout = parse_timeout_secs(values.things_creation_timeout_secs)?;
        let things_api_port = parse_api_port(values.things_api_port)?;

        Self::from_db_path_with_write_config(
            PathBuf::from(value),
            values
                .whisper_inference_url
                .unwrap_or_else(|| DEFAULT_WHISPER_INFERENCE_URL.to_string()),
            things_creation_timeout,
            values
                .things_api_host
                .unwrap_or_else(|| DEFAULT_THINGS_API_HOST.to_string()),
            things_api_port,
        )
    }

    #[cfg(test)]
    pub fn from_db_path(path: PathBuf) -> Result<Self, ConfigError> {
        Self::from_db_path_with_write_config(
            path,
            DEFAULT_WHISPER_INFERENCE_URL.to_string(),
            Duration::from_secs(DEFAULT_THINGS_CREATION_TIMEOUT_SECS),
            DEFAULT_THINGS_API_HOST.to_string(),
            DEFAULT_THINGS_API_PORT,
        )
    }

    fn from_db_path_with_write_config(
        path: PathBuf,
        whisper_inference_url: String,
        things_creation_timeout: Duration,
        things_api_host: String,
        things_api_port: u16,
    ) -> Result<Self, ConfigError> {
        if !path.is_file() {
            return Err(ConfigError::InvalidThingsDbPath { path });
        }

        Ok(Self {
            things_db_path: path,
            whisper_inference_url,
            things_creation_timeout,
            things_api_host,
            things_api_port,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingThingsDbPath,
    EmptyThingsDbPath,
    InvalidThingsDbPath { path: PathBuf },
    InvalidThingsCreationTimeout { value: String },
    InvalidThingsApiPort { value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingThingsDbPath => write!(
                f,
                "THINGS_DB_PATH is required and must point to Things' main.sqlite file"
            ),
            Self::EmptyThingsDbPath => write!(f, "THINGS_DB_PATH must not be empty"),
            Self::InvalidThingsDbPath { path } => {
                write!(
                    f,
                    "THINGS_DB_PATH does not point to a readable file: {}",
                    path.display()
                )
            }
            Self::InvalidThingsCreationTimeout { value } => write!(
                f,
                "THINGS_CREATION_TIMEOUT_SECS must be a positive integer, got {value:?}"
            ),
            Self::InvalidThingsApiPort { value } => {
                write!(f, "THINGS_API_PORT must be a valid TCP port, got {value:?}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Default)]
struct ConfigValues {
    things_db_path: Option<OsString>,
    whisper_inference_url: Option<String>,
    things_creation_timeout_secs: Option<String>,
    things_api_host: Option<String>,
    things_api_port: Option<String>,
}

trait OsStringExt {
    fn is_empty(&self) -> bool;
}

impl OsStringExt for OsString {
    fn is_empty(&self) -> bool {
        Path::new(self).as_os_str().is_empty()
    }
}

#[derive(Debug, Default)]
struct DotenvValues {
    values: Vec<(String, String)>,
}

impl DotenvValues {
    fn read() -> Self {
        let contents = fs::read_to_string(".env").unwrap_or_default();
        Self::parse(&contents)
    }

    fn parse(contents: &str) -> Self {
        Self {
            values: contents.lines().filter_map(parse_dotenv_line).collect(),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }
}

#[cfg(test)]
fn parse_dotenv_things_db_path_line(line: &str) -> Option<String> {
    parse_dotenv_key_line(line, "THINGS_DB_PATH")
}

#[cfg(test)]
fn parse_dotenv_key_line(line: &str, expected_key: &str) -> Option<String> {
    let (key, value) = parse_dotenv_line(line)?;
    if key == expected_key {
        Some(value)
    } else {
        None
    }
}

fn parse_dotenv_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    Some((
        key.to_string(),
        unquote_dotenv_value(value.trim()).to_string(),
    ))
}

fn unquote_dotenv_value(value: &str) -> &str {
    if value.len() < 2 {
        return value;
    }

    let bytes = value.as_bytes();
    let first = bytes[0];
    let last = bytes[value.len() - 1];
    if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn read_config_os_value(key: &str, dotenv: &DotenvValues) -> Option<OsString> {
    env::var_os(key).or_else(|| dotenv.get(key).map(OsString::from))
}

fn read_config_value(key: &str, dotenv: &DotenvValues) -> Option<String> {
    env::var(key)
        .ok()
        .or_else(|| dotenv.get(key).map(str::to_string))
}

fn parse_timeout_secs(value: Option<String>) -> Result<Duration, ConfigError> {
    let Some(value) = value else {
        return Ok(Duration::from_secs(DEFAULT_THINGS_CREATION_TIMEOUT_SECS));
    };

    let seconds = value
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidThingsCreationTimeout {
            value: value.clone(),
        })?;

    if seconds == 0 {
        return Err(ConfigError::InvalidThingsCreationTimeout { value });
    }

    Ok(Duration::from_secs(seconds))
}

fn parse_api_port(value: Option<String>) -> Result<u16, ConfigError> {
    let Some(value) = value else {
        return Ok(DEFAULT_THINGS_API_PORT);
    };

    value
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidThingsApiPort { value })
}

#[cfg(test)]
mod tests;
