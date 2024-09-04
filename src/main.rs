use crate::common::config::ApiConfig;
use axum::extract::{MatchedPath, Path, State};
use axum::http::{Request, StatusCode};
use axum::{routing::get, Router};
use chrono::{DateTime, Utc};
use dotenv::dotenv;
use reqwest::Url;
use std::env;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::info_span;
use tracing::Level;

mod websocket_parse;
use crate::websocket_parse::{build_tags_map, get_services_info, TagMap, TagName};

pub mod common;
use self::common::config::KumaConnectionConfig;

type PrometheusFormattedMetrics = String;

/// Request metrics info from kuma (uptime checker) service
///
/// Actually it is wrapper for proxy call
///
/// # Arguments
///
/// * `authorized_url`: url with basic jwt included
///
/// returns: Result<String, Error>
///
async fn get_kuma_metrics(
    authorized_url: Url,
) -> Result<PrometheusFormattedMetrics, reqwest::Error> {
    let response = reqwest::get(authorized_url).await;

    match response {
        Ok(response) => Ok(response.text().await.unwrap()),
        Err(err) => Err(err),
    }
}

#[derive(Debug)]
enum ServiceError {
    UnknownTag(String),
}

impl Display for ServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::UnknownTag(text) => {
                write!(f, "{}", text)
            }
        }
    }
}

/// Filters metrics that has not belongs to specified tag
///
/// Still keeps comments and rest prometheus info (hints, types, even for empty metrics)
///
/// # Arguments
///
/// * `metrics`: String with fetched metrics
/// * `passed_tag`: tag to filter metrics
/// * `tag_map`: info about what service (which names are using in metrics) which tag belongs
///
/// returns: Result<String, ServiceError>
fn filter_metrics(
    metrics: PrometheusFormattedMetrics,
    passed_tag: TagName,
    tag_map: TagMap,
) -> Result<PrometheusFormattedMetrics, ServiceError> {
    let mut lines: Vec<String> = vec![];
    for line in metrics.split("\n").collect::<Vec<&str>>() {
        if line.starts_with(&['\n', '#']) || line.trim().is_empty() {
            lines.push(line.to_string());
            continue;
        }

        let matched_services = tag_map.get(&passed_tag);
        if matched_services.is_none() {
            return Err(ServiceError::UnknownTag(format!(
                "No matched services for tag \"{}\"",
                passed_tag
            )));
        }
        for service_name in matched_services.unwrap() {
            if line.contains(format!("monitor_name=\"{}\",", service_name).as_str()) {
                lines.push(line.to_string());
            }
        }
    }
    return Ok(lines.join("\n"));
}

async fn update_tags_mapping(state: AppState, shared_state: SharedAppState) {
    let services_response = get_services_info(&state.kuma_config, 5.).await;
    match services_response {
        Ok(services) => {
            drop(state);
            shared_state.write().await.tags_map = build_tags_map(services);
            shared_state.write().await.update_at = Utc::now();
        }
        Err(err) => {
            tracing::error!(
                    "Failed to fetch tags info from {} (will use old version bumped at {}). reason: {:#?}",
                    state.kuma_config.url,state.update_at, err
                );
        }
    }
}

async fn get_filtered_metrics(
    State(shared_state): State<SharedAppState>,
    tag: Option<Path<String>>,
) -> (StatusCode, String) {
    // TODO: pass auth info from url instead of config
    let state_lock = shared_state.read().await;
    let state = state_lock.clone();
    drop(state_lock);

    if (Utc::now() - state.update_at).num_seconds() > state.api_config.tags_ttl_seconds.into() {
        tracing::info!("Tags mapping expired, fetching new...");
        update_tags_mapping(state.clone(), shared_state).await
    }

    let metrics = get_kuma_metrics(state.kuma_config.full_url.clone()).await;

    if metrics.is_err() {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Failed to fetch data from {}. Err: {}",
                state.kuma_config.url,
                metrics.err().unwrap().to_string()
            ),
        );
    }

    if tag.is_none() {
        // TODO logging
        return (StatusCode::OK, metrics.unwrap());
    }

    let filtered_metrics = filter_metrics(metrics.unwrap(), tag.unwrap().0, state.tags_map.clone());

    if filtered_metrics.is_err() {
        let err_msg = filtered_metrics.err().unwrap().to_string();
        return (
            StatusCode::BAD_REQUEST,
            format!("Failed to filter metrics. Reason: {}", err_msg),
        );
    }
    return (StatusCode::OK, filtered_metrics.unwrap().to_string());
}

#[derive(Clone)]
struct AppState {
    kuma_config: KumaConnectionConfig,
    api_config: ApiConfig,
    tags_map: TagMap,
    update_at: DateTime<Utc>,
}

type SharedAppState = Arc<RwLock<AppState>>;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();
    // TODO access/err log

    let kuma_config = KumaConnectionConfig::new();
    let api_config = ApiConfig::new();

    let services_response = get_services_info(&kuma_config, 5.).await;
    if services_response.is_err() {
        tracing::error!(
            "Failed to fetch tags info from {}. reason: {:#?}",
            kuma_config.url,
            services_response.err().unwrap()
        );
        panic!("Failed to fetch tags from kuma service");
    }
    let services = services_response.unwrap();
    let tags_map = build_tags_map(services);

    let addr = format!("{}:{}", api_config.host.clone(), api_config.port.clone());
    let routes = Router::new()
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                // Log the matched route's path (with placeholders not filled in).
                // Use request.uri() or OriginalUri if you want the real path.
                let matched_path = request
                    .extensions()
                    .get::<MatchedPath>()
                    .map(MatchedPath::as_str);

                info_span!(
                    "http_request",
                    method = ?request.method(),
                    matched_path,
                    some_other_field = tracing::field::Empty,
                )
            }),
        )
        .route("/:tag", get(get_filtered_metrics))
        .route("/", get(get_filtered_metrics))
        .with_state(Arc::new(RwLock::new(AppState {
            kuma_config,
            tags_map,
            update_at: Utc::now(),
            api_config,
        })));

    tracing::info!("Starting server at: {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, routes).await.unwrap();
}
