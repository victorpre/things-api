mod app;
mod audio;
mod config;
mod db;
mod routes;
mod things;
mod things_url;

use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::Config::from_env()?;
    let pool = db::connect(&config).await?;
    let repository = things::ThingsRepository::new(pool);
    let whisper_transcriber = Arc::new(audio::WhisperClient::new(
        config.whisper_inference_url.clone(),
    ));
    let things_task_writer = Arc::new(things_url::ThingsUrlClient::new(
        config.things_auth_token.clone(),
    ));
    let state =
        app::AppState::with_write_services(repository, whisper_transcriber, things_task_writer);
    let bind_address = format!("{}:{}", config.things_api_host, config.things_api_port);
    let listener = tokio::net::TcpListener::bind(&bind_address).await?;

    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
