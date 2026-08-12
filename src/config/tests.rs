use super::*;
use std::{fs::File, time::SystemTime};

#[test]
fn reports_missing_db_path() {
    let error = Config::from_env_value(None).unwrap_err();

    assert_eq!(error, ConfigError::MissingThingsDbPath);
}

#[test]
fn reports_empty_db_path() {
    let error = Config::from_env_value(Some(OsString::new())).unwrap_err();

    assert_eq!(error, ConfigError::EmptyThingsDbPath);
}

#[test]
fn reports_nonexistent_db_path() {
    let path = unique_temp_path("missing-main.sqlite");
    let error = Config::from_db_path(path.clone()).unwrap_err();

    assert_eq!(error, ConfigError::InvalidThingsDbPath { path });
}

#[test]
fn accepts_existing_db_path() {
    let path = unique_temp_path("main.sqlite");
    File::create(&path).unwrap();

    let config = Config::from_db_path(path.clone()).unwrap();

    assert_eq!(config.things_db_path, path);
    std::fs::remove_file(config.things_db_path).unwrap();
}

#[test]
fn parses_db_path_from_dotenv_line() {
    assert_eq!(
        parse_dotenv_things_db_path_line(
            "THINGS_DB_PATH=\"/Users/example/Things Database.thingsdatabase/main.sqlite\""
        )
        .as_deref(),
        Some("/Users/example/Things Database.thingsdatabase/main.sqlite")
    );
}

#[test]
fn ignores_non_matching_dotenv_lines() {
    assert_eq!(parse_dotenv_things_db_path_line("# THINGS_DB_PATH=x"), None);
    assert_eq!(parse_dotenv_things_db_path_line("OTHER=x"), None);
}

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    env::temp_dir().join(format!(
        "things-api-config-test-{}-{nanos}-{name}",
        std::process::id()
    ))
}
