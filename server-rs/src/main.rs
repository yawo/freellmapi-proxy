pub mod db;
pub mod models;
pub mod crypto;
pub mod services;
pub mod providers;
pub mod proxy;
pub mod dashboard_api;

use axum::{routing::post, Router};
use sea_orm::Database;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use proxy::{chat_completions, AppState};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting FreeLLMAPI Rust Backend...");

    // Initialize crypto from .env
    // Try to load from current dir, if fails try parent dir
    if dotenvy::dotenv().is_err() {
        dotenvy::from_path("../.env").ok();
    }
    crypto::init_encryption_key();

    // Connect to DB
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://./freeapi.db".to_string());
    println!("Connecting to DB: {}", db_url);
    let db = Database::connect(&db_url).await?;

    let state = AppState { db: db.clone() };

    // Start background health checker
    tokio::spawn(services::health::run_health_checks(db));

    // Build Router
    let client_dist = std::env::var("CLIENT_DIST").unwrap_or_else(|_| "../client/dist".to_string());
    println!("Serving frontend from: {}", client_dist);

    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .nest("/api", dashboard_api::api_router())
        .fallback_service(
            ServeDir::new(&client_dist)
                .not_found_service(ServeFile::new(format!("{}/index.html", client_dist)))
        )
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3001));
    println!("Listening on http://{}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
