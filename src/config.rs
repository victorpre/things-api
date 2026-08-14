use std::{
    env,
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Config {
    pub things_db_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_value(env::var_os("THINGS_DB_PATH").or_else(read_dotenv_things_db_path))
    }

    fn from_env_value(value: Option<OsString>) -> Result<Self, ConfigError> {
        let value = value.ok_or(ConfigError::MissingThingsDbPath)?;

        if value.is_empty() {
            return Err(ConfigError::EmptyThingsDbPath);
        }

        Self::from_db_path(PathBuf::from(value))
    }

    pub fn from_db_path(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.is_file() {
            return Err(ConfigError::InvalidThingsDbPath { path });
        }

        Ok(Self {
            things_db_path: path,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    MissingThingsDbPath,
    EmptyThingsDbPath,
    InvalidThingsDbPath { path: PathBuf },
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
        }
    }
}

impl std::error::Error for ConfigError {}

trait OsStringExt {
    fn is_empty(&self) -> bool;
}

impl OsStringExt for OsString {
    fn is_empty(&self) -> bool {
        Path::new(self).as_os_str().is_empty()
    }
}

fn read_dotenv_things_db_path() -> Option<OsString> {
    let contents = fs::read_to_string(".env").ok()?;

    contents
        .lines()
        .find_map(parse_dotenv_things_db_path_line)
        .map(OsString::from)
}

fn parse_dotenv_things_db_path_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let (key, value) = line.split_once('=')?;
    if key.trim() != "THINGS_DB_PATH" {
        return None;
    }

    Some(unquote_dotenv_value(value.trim()).to_string())
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

#[cfg(test)]
mod tests;
