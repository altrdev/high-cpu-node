use crate::endpoints::{STATUS_MAX, STATUS_MIN};
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

// What JavaScript's Number() makes of a path segment, when it is an integer
fn integer(text: &str) -> Option<i64> {
    let value: f64 = text.trim().parse().ok()?;
    (value.is_finite() && value.fract() == 0.0).then_some(value as i64)
}

// Answers with the requested status code, to test probes and retries
pub async fn handler(Path(code): Path<String>) -> Response {
    let code = integer(&code).filter(|code| (STATUS_MIN..=STATUS_MAX).contains(code));
    let Some(code) = code else {
        return (StatusCode::BAD_REQUEST, format!("code must be an integer between {STATUS_MIN} and {STATUS_MAX}")).into_response();
    };
    let status = StatusCode::from_u16(code as u16).expect("a code between 200 and 599");
    // these statuses can't carry a body
    let body = if matches!(code, 204 | 205 | 304) { String::new() } else { format!("Status: {code}") };
    (status, body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_only() {
        assert_eq!(integer("503"), Some(503));
        assert_eq!(integer("abc"), None);
        assert_eq!(integer("2.5"), None);
        assert_eq!(integer("1e2"), Some(100));
    }
}
