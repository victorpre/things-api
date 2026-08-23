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
    assert_eq!(
        config.whisper_inference_url,
        "http://127.0.0.1:8080/inference"
    );
    assert_eq!(config.things_api_host, "127.0.0.1");
    assert_eq!(config.things_api_port, 3000);
    assert_eq!(config.things_auth_token, None);
    std::fs::remove_file(config.things_db_path).unwrap();
}

#[test]
fn accepts_write_route_config_values() {
    let path = unique_temp_path("main.sqlite");
    File::create(&path).unwrap();

    let config = Config::from_raw(ConfigValues {
        things_db_path: Some(path.clone().into_os_string()),
        whisper_inference_url: Some("http://127.0.0.1:9090/inference".to_string()),
        things_api_host: Some("0.0.0.0".to_string()),
        things_api_port: Some("3001".to_string()),
        things_auth_token: Some("secret-token".to_string()),
    })
    .unwrap();

    assert_eq!(config.things_db_path, path);
    assert_eq!(
        config.whisper_inference_url,
        "http://127.0.0.1:9090/inference"
    );
    assert_eq!(config.things_api_host, "0.0.0.0");
    assert_eq!(config.things_api_port, 3001);
    assert_eq!(config.things_auth_token.as_deref(), Some("secret-token"));
    std::fs::remove_file(config.things_db_path).unwrap();
}

#[test]
fn rejects_invalid_api_port() {
    let path = unique_temp_path("main.sqlite");
    File::create(&path).unwrap();

    let error = Config::from_raw(ConfigValues {
        things_db_path: Some(path.clone().into_os_string()),
        things_api_port: Some("not-a-port".to_string()),
        ..ConfigValues::default()
    })
    .unwrap_err();

    assert_eq!(
        error,
        ConfigError::InvalidThingsApiPort {
            value: "not-a-port".to_string()
        }
    );
    std::fs::remove_file(path).unwrap();
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

#[test]
fn parses_write_route_dotenv_lines() {
    let dotenv = DotenvValues::parse(
        r#"
        THINGS_DB_PATH="/tmp/main.sqlite"
        WHISPER_INFERENCE_URL="http://127.0.0.1:8080/inference"
        THINGS_API_HOST="0.0.0.0"
        THINGS_API_PORT="3001"
        THINGS_AUTH_TOKEN="secret-token"
        "#,
    );

    assert_eq!(dotenv.get("THINGS_DB_PATH"), Some("/tmp/main.sqlite"));
    assert_eq!(
        dotenv.get("WHISPER_INFERENCE_URL"),
        Some("http://127.0.0.1:8080/inference")
    );
    assert_eq!(dotenv.get("THINGS_API_HOST"), Some("0.0.0.0"));
    assert_eq!(dotenv.get("THINGS_API_PORT"), Some("3001"));
    assert_eq!(dotenv.get("THINGS_AUTH_TOKEN"), Some("secret-token"));
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
