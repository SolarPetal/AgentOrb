use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use agent_orb_core::event::EventEnvelope;

use crate::{security, session_store::SessionStore};

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<SessionStore>>,
    token: Arc<String>,
}

impl AppState {
    #[cfg(test)]
    pub fn new(token: String) -> Self {
        Self::with_completed_hold_seconds(token, 10)
    }

    pub fn with_completed_hold_seconds(token: String, completed_hold_seconds: u64) -> Self {
        Self {
            store: Arc::new(Mutex::new(SessionStore::with_completed_hold_seconds(
                completed_hold_seconds,
            ))),
            token: Arc::new(token),
        }
    }
}

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/events", post(post_event))
        .route("/v1/status", get(get_status))
        .route("/v1/status/clear", post(clear_status))
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn post_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers, &state)?;

    let input = std::str::from_utf8(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let event = EventEnvelope::from_json_str(input).map_err(|_| StatusCode::BAD_REQUEST)?;

    let mut store = state.store.lock().await;
    store.apply_event(event);

    Ok(Json(json!({ "ok": true })))
}

async fn get_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::session_store::StatusSnapshot>, StatusCode> {
    require_auth(&headers, &state)?;

    let store = state.store.lock().await;
    Ok(Json(store.global_status()))
}

async fn clear_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    require_auth(&headers, &state)?;

    let mut store = state.store.lock().await;
    let cleared = store.clear_terminal_statuses();

    Ok(Json(json!({ "ok": true, "cleared": cleared })))
}

fn require_auth(headers: &HeaderMap, state: &AppState) -> Result<(), StatusCode> {
    if security::is_authorized(headers, state.token.as_str()) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_orb_core::status::InternalStatus;
    use axum::{
        body::{to_bytes, Body},
        http::{header::CONTENT_TYPE, Method, Request},
    };
    use serde_json::Value;
    use tower::ServiceExt;

    const TOKEN: &str = "test-token";

    fn app() -> Router {
        build_app(AppState::new(TOKEN.to_string()))
    }

    fn auth_value() -> String {
        format!("Bearer {TOKEN}")
    }

    async fn json_body(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body should be readable");
        serde_json::from_slice(&bytes).expect("body should be json")
    }

    #[tokio::test]
    async fn health_is_public() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["ok"], true);
    }

    #[tokio::test]
    async fn write_endpoint_requires_authorization() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/events")
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(include_str!(
                        "../../../examples/events/session-started.json"
                    )))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_endpoint_requires_authorization() {
        let response = app()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/status")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn events_update_status() {
        let app = app();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/events")
                    .header("Authorization", auth_value())
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(include_str!(
                        "../../../examples/events/session-started.json"
                    )))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/events")
                    .header("Authorization", auth_value())
                    .header(CONTENT_TYPE, "application/json")
                    .body(Body::from(include_str!(
                        "../../../examples/events/output-received.json"
                    )))
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/status")
                    .header("Authorization", auth_value())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["status"], "active");
        assert_eq!(body["visual"], "blue_spinning");
        assert_eq!(body["source"], "codex");
    }

    #[tokio::test]
    async fn clear_removes_failed_global_status() {
        let app = app();

        for event in [
            include_str!("../../../examples/events/session-started.json"),
            include_str!("../../../examples/events/process-exited.json"),
        ] {
            let body = if event.contains("process.exited") {
                event.replace("\"exit_code\": 0", "\"exit_code\": 1")
            } else {
                event.to_string()
            };

            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/v1/events")
                        .header("Authorization", auth_value())
                        .header(CONTENT_TYPE, "application/json")
                        .body(Body::from(body))
                        .expect("request should build"),
                )
                .await
                .expect("request should succeed");
            assert_eq!(response.status(), StatusCode::OK);
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/status")
                    .header("Authorization", auth_value())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(json_body(response).await["status"], "failed");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/status/clear")
                    .header("Authorization", auth_value())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/status")
                    .header("Authorization", auth_value())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("request should succeed");
        let body = json_body(response).await;
        assert_eq!(body["status"], serde_json::json!(InternalStatus::Idle));
    }
}
