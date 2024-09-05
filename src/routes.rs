use axum::extract::path::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum_auth::AuthBasic;
use chrono::Utc;

use crate::{filter_metrics, get_kuma_metrics, SharedAppState, update_tags_mapping};
use crate::utils::build_url_with_auth;

pub async fn get_filtered_metrics(
    State(shared_state): State<SharedAppState>,
    tag: Option<Path<String>>,
    AuthBasic((_, token)): AuthBasic,
) -> (StatusCode, String) {
    let state_lock = shared_state.read().await;
    let state = state_lock.clone();
    drop(state_lock);

    if (Utc::now() - state.update_at).num_seconds() > state.api_config.tags_ttl_seconds.into() {
        tracing::info!("Tags mapping expired, fetching new...");
        update_tags_mapping(state.clone(), shared_state).await
    }

    let metrics = get_kuma_metrics(
        build_url_with_auth(&state.kuma_config.url, token.unwrap_or(String::new()).as_str())
    ).await;

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