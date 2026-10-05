//! The page a sign-in that failed ends on, and the one for an address that isn't a route. They
//! look like the web app's, with the colours of `packages/theme`.

use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{Html, IntoResponse, Response};

const TOKENS: &str = include_str!("../../../packages/theme/tokens.css");
const STYLE: &str = include_str!("../assets/page.css");
const FONT: &[u8] = include_bytes!("../assets/dm-sans.woff2");
const LOGO: &str = include_str!("../assets/logo.svg");
const THEME_SCRIPT: &str =
    r#"document.documentElement.classList.toggle("dark", !document.cookie.includes("theme=light"))"#;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn message(title: &str, text: &str) -> Html<String> {
    let (title, text) = (escape(title), escape(text));
    Html(format!(
        r#"<!doctype html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<script>{THEME_SCRIPT}</script>
<title>{title} · Motile</title>
<link rel="icon" type="image/svg+xml" href="https://motile.app/favicon.svg">
<style>{TOKENS}{STYLE}</style>
</head>
<body>
<header><a href="https://motile.app">{LOGO}Motile</a></header>
<main><h1>{title}</h1><p>{text}</p></main>
</body>
</html>
"#
    ))
}

pub async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, message("Not found", "There is nothing at this address.")).into_response()
}

pub async fn font() -> Response {
    ([(CONTENT_TYPE, "font/woff2"), (CACHE_CONTROL, "public, max-age=604800")], FONT).into_response()
}
