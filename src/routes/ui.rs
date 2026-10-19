use axum::http::header;
use axum::response::IntoResponse;

/// The control panel: one static page that reads `/info` and `/metrics` and calls the other
/// endpoints. Only mounted with `UI=1`, since it makes `/crash` a button.
pub async fn handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], include_str!("../../assets/ui.html"))
}
