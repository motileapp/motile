mod api;
mod config;
mod db;
#[cfg(test)]
mod e2e;
mod error;
mod google;
mod pages;
mod sign_in;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::http::HeaderValue;
use axum::http::header::{REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS};
use axum::routing::{delete, get, post};
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;

pub struct Inner {
    pub config: Config,
    pub db: sqlx::PgPool,
    pub http: reqwest::Client,
}

pub type AppState = Arc<Inner>;

fn router(state: AppState) -> Router {
    let header = |name, value| SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value));
    let router = Router::new()
        .route("/install", get(pages::install_script))
        .route("/download/{file}", get(pages::download))
        .route("/healthz", get(|| async { "ok" }))
        .route("/auth/start", get(sign_in::start))
        .route("/auth/google/callback", get(sign_in::google_callback))
        .route("/api/auth/exchange", post(api::exchange))
        .route("/api/sessions", post(api::create_session))
        .route("/api/sessions/current", delete(api::end_session))
        .route("/api/dev/login", post(api::dev_login))
        .route("/api/me", get(api::me))
        .route("/api/enroll-tokens", post(api::create_enroll_token))
        .route("/api/enroll", post(api::enroll))
        .route("/api/devices/{public_key}", delete(api::remove_device));
    let router = match &state.config.marketing_dir {
        Some(marketing_dir) => {
            let not_found = ServeFile::new(std::path::Path::new(marketing_dir).join("404.html"));
            router.fallback_service(ServeDir::new(marketing_dir).not_found_service(not_found))
        }
        None => router.fallback(pages::not_found),
    };
    router
        .layer(header(X_CONTENT_TYPE_OPTIONS, "nosniff"))
        .layer(header(X_FRAME_OPTIONS, "DENY"))
        .layer(header(REFERRER_POLICY, "no-referrer"))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl-c handler") };
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    motile_protocol::tls::install();
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("config error: {err}");
        std::process::exit(1);
    });
    if config.dev_login {
        tracing::warn!("DEV_LOGIN is on: anyone can sign in as anyone. Never set it in production.");
    }
    let db = PgPoolOptions::new().max_connections(10).connect(&config.database_url).await.expect("connect to Postgres");
    sqlx::migrate!().run(&db).await.expect("run migrations");
    let http = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().expect("HTTP client");
    let state = Arc::new(Inner { config, db, http });

    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(err) = db::cleanup(&cleanup_state.db).await {
                tracing::warn!("cleanup failed: {err}");
            }
        }
    });

    let address = format!("0.0.0.0:{}", state.config.port);
    let listener = tokio::net::TcpListener::bind(&address).await.expect("bind port");
    tracing::info!("Motile auth listening on {address} as {}", state.config.public_url);
    axum::serve(listener, router(state)).with_graceful_shutdown(shutdown_signal()).await.expect("server");
}
