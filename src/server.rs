use anyhow::Context;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::config::Config;
use crate::report::{self, ReportError, SuggestRequest, SuggestResponse};
use crate::rules::RuleEngine;
use crate::species;

struct AppState {
    client: reqwest::Client,
    engines: HashMap<&'static str, RuleEngine>,
    gauge_radius_mi: f64,
    rule_top_n: usize,
}

pub async fn run(port: u16, config: Config) -> anyhow::Result<()> {
    let mut engines = HashMap::new();
    for profile in species::all() {
        let engine = RuleEngine::load(profile.rules_toml)
            .with_context(|| format!("failed to load rules for species {}", profile.id))?;
        engines.insert(profile.id, engine);
    }

    let state = Arc::new(AppState {
        client: reqwest::Client::new(),
        engines,
        gauge_radius_mi: config.usgs_gauge_radius_mi,
        rule_top_n: config.rule_top_n,
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/species", get(list_species))
        .route("/api/v1/suggest", post(suggest))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("fishcast listening on {addr}");

    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;

    Ok(())
}

async fn healthz() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct SpeciesInfo {
    id: &'static str,
    display_name: &'static str,
}

async fn list_species() -> Json<Vec<SpeciesInfo>> {
    Json(
        species::all()
            .iter()
            .map(|p| SpeciesInfo { id: p.id, display_name: p.display_name })
            .collect(),
    )
}

async fn suggest(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SuggestRequest>,
) -> Result<Json<SuggestResponse>, AppError> {
    let response = report::suggest(&state.client, &state.engines, state.gauge_radius_mi, state.rule_top_n, req).await?;
    Ok(Json(response))
}

/// Every external condition failure degrades to Unknown inside
/// conditions::resolve rather than erroring, so the only things that reach
/// here are genuine client mistakes - an unknown species or an
/// unresolvable location (e.g. a bad zip code) - both 400. There is no
/// generic 500/502 path by design.
struct AppError(ReportError);

impl From<ReportError> for AppError {
    fn from(e: ReportError) -> Self {
        AppError(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (StatusCode::BAD_REQUEST, body).into_response()
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
