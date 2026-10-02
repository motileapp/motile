//! The page a sign-in that failed ends on, and the one for an address that isn't a route.

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

const STYLE: &str = include_str!("../assets/message.css");

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn message(title: &str, text: &str) -> Html<String> {
    let (title, text) = (escape(title), escape(text));
    Html(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n\
         <title>{title} · Motile</title>\n<style>{STYLE}</style>\n</head>\n\
         <body>\n<main><b>Motile</b><h1>{title}</h1><p>{text}</p></main>\n</body>\n</html>\n"
    ))
}

pub async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, message("Not found", "There is nothing at this address.")).into_response()
}
