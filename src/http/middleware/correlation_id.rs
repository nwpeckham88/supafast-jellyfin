use axum::{
    extract::FromRequestParts,
    http::{header::HeaderName, request::Parts, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use uuid::Uuid;

const CORRELATION_ID_HEADER: &str = "x-correlation-id";

#[derive(Debug, Clone)]
pub struct CorrelationId(String);

impl CorrelationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for CorrelationId
where
    S: Send + Sync,
{
    type Rejection = (axum::http::StatusCode, String);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<CorrelationId>().cloned().ok_or((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "missing correlation id".to_string(),
        ))
    }
}

pub async fn correlation_id_middleware(mut req: Request<axum::body::Body>, next: Next) -> Response {
    let correlation_id = extract_or_generate(req.headers().get(CORRELATION_ID_HEADER));
    req.extensions_mut().insert(correlation_id.clone());

    let span = tracing::info_span!("http.request", correlation_id = %correlation_id);
    let mut response = next.run(req).instrument(span).await;
    let header_name = HeaderName::from_static(CORRELATION_ID_HEADER);

    if let Ok(value) = HeaderValue::from_str(correlation_id.as_str()) {
        response.headers_mut().insert(header_name, value);
    }

    response
}

fn extract_or_generate(raw_header: Option<&HeaderValue>) -> CorrelationId {
    let maybe_valid = raw_header
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return None;
            }
            if Uuid::parse_str(trimmed).is_ok() {
                return Some(trimmed.to_string());
            }
            None
        });

    CorrelationId(maybe_valid.unwrap_or_else(|| Uuid::new_v4().to_string()))
}
