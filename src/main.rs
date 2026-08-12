mod app;
mod config;
mod db;
mod routes;
mod things;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::Config::from_env()?;
    let pool = db::connect(&config).await?;
    let repository = things::ThingsRepository::new(pool);
    let state = app::AppState::new(repository);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    axum::serve(listener, app::router(state)).await?;

    Ok(())
}
