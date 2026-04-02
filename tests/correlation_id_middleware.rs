use axum::{
    body::Body,
    http::Request,
    middleware,
    response::IntoResponse,
    routing::get,
    Router,
};
use tower::ServiceExt;
use uuid::Uuid;

use supafast_jellyfin::http::middleware::correlation_id::{
    correlation_id_middleware,
    CorrelationId,
};

async fn handler(correlation_id: CorrelationId) -> impl IntoResponse {
    correlation_id.to_string()
}

#[tokio::test]
async fn generates_correlation_id_when_missing() {
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn(correlation_id_middleware));

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).expect("request"))
        .await
        .expect("response");

    let header = response
        .headers()
        .get("x-correlation-id")
        .expect("header")
        .to_str()
        .expect("header value");

    assert!(Uuid::parse_str(header).is_ok());
}

#[tokio::test]
async fn preserves_valid_incoming_correlation_id() {
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn(correlation_id_middleware));

    let incoming = Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-correlation-id", &incoming)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let header = response
        .headers()
        .get("x-correlation-id")
        .expect("header")
        .to_str()
        .expect("header value");

    assert_eq!(header, incoming);
}

#[tokio::test]
async fn extension_and_response_header_match() {
    let app = Router::new()
        .route("/", get(handler))
        .layer(middleware::from_fn(correlation_id_middleware));

    let incoming = Uuid::new_v4().to_string();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-correlation-id", &incoming)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let header = response
        .headers()
        .get("x-correlation-id")
        .expect("header")
        .to_str()
        .expect("header value")
        .to_string();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = String::from_utf8(body.to_vec()).expect("utf8 body");

    assert_eq!(header, body);
}
