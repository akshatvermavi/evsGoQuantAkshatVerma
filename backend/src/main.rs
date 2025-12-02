mod config;
mod session_manager;
mod delegation_manager;
mod auto_deposit;
mod vault_monitor;
mod transaction_signer;
mod api;

use anyhow::Result;
use axum::{routing::{get, post, delete}, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "backend=info,axum=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = config::Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    let shared_state = api::AppState::new(pool, cfg.clone()).await?;

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/session/create", post(api::create_session))
        .route("/session/approve", post(api::approve_session))
        .route("/session/revoke", delete(api::revoke_session))
        .route("/session/status", get(api::session_status))
        .route("/session/deposit", post(api::session_deposit))
        .route("/ws/session", get(api::session_ws))
        .with_state(shared_state);

    let addr: SocketAddr = cfg.listen_addr.parse()?;
    tracing::info!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("failed to install signal handler");
        term.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
