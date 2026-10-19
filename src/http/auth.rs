//! `TOKEN`: the endpoints that change the state of the instance ask for `Authorization: Bearer <token>`.
use crate::app::AppState;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtle::ConstantTimeEq;

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

/// Whether `header` is `Bearer <token>`. The scheme is case-insensitive and the token is
/// compared through a digest, so the comparison takes the same time whatever the value is.
fn authorized(header: &str, token: &str) -> bool {
    let mut parts = header.split(' ');
    let scheme = parts.next().unwrap_or("");
    let value = parts.next().unwrap_or("");
    scheme.eq_ignore_ascii_case("bearer") && bool::from(digest(value).ct_eq(&digest(token)))
}

/// With a token set the request must carry it, or it is answered with a `401`. Without one
/// nothing is checked.
pub async fn guard(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let token = &state.config.token;
    if token.is_empty() {
        return next.run(request).await;
    }
    let sent = request.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).unwrap_or("");
    if authorized(sent, token) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
        "Unauthorized: send the token as \"Authorization: Bearer <token>\"",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_right_bearer_token_passes() {
        assert!(authorized("Bearer s3cret", "s3cret"));
        assert!(authorized("bearer s3cret", "s3cret"));
        assert!(!authorized("Bearer wrong", "s3cret"));
        assert!(!authorized("Basic s3cret", "s3cret"));
        assert!(!authorized("s3cret", "s3cret"));
        assert!(!authorized("", "s3cret"));
        assert!(!authorized("Bearer  s3cret", "s3cret"), "two spaces make the token empty");
    }
}
