use std::sync::Arc;

use axum::{Router, routing::get};
use axum::extract::{MatchedPath};
use axum::http::{Request};
use chrono::{DateTime, Utc};
use dotenv::dotenv;
use reqwest::Url;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info_span;
use tracing::Level;
use tracing_subscriber::EnvFilter;

use crate::common::config::ApiConfig;
use crate::errors::ServiceError;
use crate::routes::get_filtered_metrics;
use crate::websocket_parse::{build_tags_map, get_services_info, TagMap, TagName};

use self::common::config::KumaConnectionConfig;

mod websocket_parse;
pub mod common;
mod routes;
mod errors;
mod utils;

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

#[derive(Clone)]
struct AppState {
    kuma_config: KumaConnectionConfig,
    api_config: ApiConfig,
    tags_map: TagMap,
    update_at: DateTime<Utc>,
}

type SharedAppState = Arc<RwLock<AppState>>;

// async fn log_requests(
//     req: Request<Body>,
//     next: Next,
// ) -> Result<Response<Body>, axum::Error> {
//     let start = Instant::now();
//     let method = req.method().clone();
//     let uri = req.uri().clone();
//
//     // Proceed with the request
//     let response = next.run(req).await;
//
//     let status = response.status().as_u16();
//     let duration = start.elapsed();
//
//     // Log access information
//     tracing::info!(
//         method = %method,
//         uri = %uri,
//         status = status,
//         duration = ?duration,
//         "Request completed"
//     );
//
//     // Log error if status code is 4xx or 5xx
//     if status >= 400 {
//         tracing::error!(
//             method = %method,
//             uri = %uri,
//             status = status,
//             duration = ?duration,
//             "Error occurred"
//         );
//     }
//
//     Ok(response)
// }

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .with_max_level(Level::INFO)
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
        // .layer(middleware::from_fn(log_requests))
        .route("/:tag", get(get_filtered_metrics))
        .route("/", get(get_filtered_metrics))
        .with_state(Arc::new(RwLock::new(AppState {
            kuma_config,
            tags_map,
            update_at: Utc::now(),
            api_config,
        })));

    tracing::info!("Starting server at: http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, routes).await.unwrap();
}
