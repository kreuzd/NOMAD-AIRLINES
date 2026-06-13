//! End-to-end HTTP integration tests that drive the assembled axum router via
//! `tower::ServiceExt::oneshot` (no real socket needed). These exercise the
//! auth flow, gallery CRUD authorization, and state persistence exactly as the
//! frontend hits them.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use nomad_backend::{build_router, AppState};
use serde_json::{json, Value};
use tower::ServiceExt;

/// A valid 1x1 transparent PNG as a data URL, used as image payload.
const TINY_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

fn app() -> Router {
    let state = AppState::in_memory(b"integration-test-secret").unwrap();
    // The static dir is irrelevant to /api routes; "." always exists.
    build_router(state, ".")
}

/// Send a request and return (status, parsed-json-body). Empty bodies parse to
/// `Value::Null`.
async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };

    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// Register a user and return their access token.
async fn register(app: &Router, username: &str, password: &str) -> String {
    let (status, body) = send(
        app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({ "username": username, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register failed: {body}");
    body["access_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn health_is_public() {
    let app = app();
    let (status, body) = send(&app, Method::GET, "/api/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn register_returns_token_and_user() {
    let app = app();
    let (status, body) = send(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({ "username": "alice", "password": "supersecret" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["access_token"].as_str().unwrap().len() > 10);
    assert_eq!(body["user"]["username"], "alice");
    assert!(body["user"].get("password_hash").is_none());
}

#[tokio::test]
async fn register_rejects_weak_password_and_bad_username() {
    let app = app();
    let (status, _) = send(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({ "username": "ok_user", "password": "short" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({ "username": "no spaces!", "password": "longenough" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_registration_conflicts() {
    let app = app();
    register(&app, "bob", "password123").await;
    let (status, _) = send(
        &app,
        Method::POST,
        "/api/auth/register",
        None,
        Some(json!({ "username": "BOB", "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn login_flow_and_wrong_password() {
    let app = app();
    register(&app, "carol", "password123").await;

    // wrong password
    let (status, _) = send(
        &app,
        Method::POST,
        "/api/auth/token",
        None,
        Some(json!({ "username": "carol", "password": "nope" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // correct password
    let (status, body) = send(
        &app,
        Method::POST,
        "/api/auth/login",
        None,
        Some(json!({ "username": "carol", "password": "password123" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["access_token"].is_string());
}

#[tokio::test]
async fn me_requires_valid_token() {
    let app = app();
    let token = register(&app, "dave", "password123").await;

    let (status, _) = send(&app, Method::GET, "/api/auth/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(&app, Method::GET, "/api/auth/me", Some("garbage"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, body) = send(&app, Method::GET, "/api/auth/me", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "dave");
}

#[tokio::test]
async fn image_lifecycle_create_list_get_raw_update_delete() {
    let app = app();
    let token = register(&app, "erin", "password123").await;

    // create
    let (status, body) = send(
        &app,
        Method::POST,
        "/api/images",
        Some(&token),
        Some(json!({ "name": "sketch", "data_url": TINY_PNG, "width": 1, "height": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = body["id"].as_i64().unwrap();
    assert_eq!(body["name"], "sketch");
    assert!(body["size"].as_i64().unwrap() > 0);

    // list
    let (status, body) = send(&app, Method::GET, "/api/images", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 1);

    // get (returns data_url for export)
    let (status, body) = send(
        &app,
        Method::GET,
        &format!("/api/images/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data_url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));

    // raw bytes
    let raw_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/images/{id}/raw"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(raw_resp.status(), StatusCode::OK);
    assert_eq!(raw_resp.headers()[header::CONTENT_TYPE], "image/png");
    let raw = axum::body::to_bytes(raw_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&raw[1..4], b"PNG", "served bytes are a real PNG");

    // update (rename)
    let (status, body) = send(
        &app,
        Method::PUT,
        &format!("/api/images/{id}"),
        Some(&token),
        Some(json!({ "name": "renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "renamed");

    // delete
    let (status, _) = send(
        &app,
        Method::DELETE,
        &format!("/api/images/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // gone
    let (status, _) = send(
        &app,
        Method::GET,
        &format!("/api/images/{id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn images_require_auth() {
    let app = app();
    let (status, _) = send(&app, Method::GET, "/api/images", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_cannot_access_each_others_images() {
    let app = app();
    let alice = register(&app, "alice2", "password123").await;
    let bob = register(&app, "bob2", "password123").await;

    let (_, body) = send(
        &app,
        Method::POST,
        "/api/images",
        Some(&alice),
        Some(json!({ "name": "secret", "data_url": TINY_PNG })),
    )
    .await;
    let id = body["id"].as_i64().unwrap();

    // bob cannot read, update, or delete alice's image
    for (method, has_body) in [
        (Method::GET, false),
        (Method::PUT, true),
        (Method::DELETE, false),
    ] {
        let body = has_body.then(|| json!({ "name": "hijack" }));
        let (status, _) = send(
            &app,
            method.clone(),
            &format!("/api/images/{id}"),
            Some(&bob),
            body,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "method {method} leaked image"
        );
    }

    // bob's gallery is empty
    let (_, body) = send(&app, Method::GET, "/api/images", Some(&bob), None).await;
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rejects_oversized_and_malformed_images() {
    let app = app();
    let token = register(&app, "frank", "password123").await;

    let (status, _) = send(
        &app,
        Method::POST,
        "/api/images",
        Some(&token),
        Some(json!({ "name": "bad", "data_url": "data:image/png;base64,!!!notbase64" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send(
        &app,
        Method::POST,
        "/api/images",
        Some(&token),
        Some(json!({ "name": "", "data_url": TINY_PNG })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn state_persists_per_user() {
    let app = app();
    let token = register(&app, "grace", "password123").await;

    // initially null
    let (status, body) = send(&app, Method::GET, "/api/state", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["state"].is_null());

    // save
    let (status, _) = send(
        &app,
        Method::PUT,
        "/api/state",
        Some(&token),
        Some(json!({ "state": { "openImageId": 7, "zoom": 2 } })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // restore
    let (status, body) = send(&app, Method::GET, "/api/state", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"]["openImageId"], 7);
    assert_eq!(body["state"]["zoom"], 2);
}
