//! The few pages a person sees in a browser, the installer script and the downloads.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::AppState;

const INSTALL_SCRIPT: &str = include_str!("../assets/install.sh");
const STYLE: &str = include_str!("../assets/style.css");
const HOME: &str = include_str!("../assets/home.html");
const PRIVACY: &str = include_str!("../assets/privacy.html");
const TERMS: &str = include_str!("../assets/terms.html");
const LOGO: &str = include_str!("../assets/logo.svg");

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"color-scheme\" content=\"light dark\">\n\
         <title>{title}</title>\n<link rel=\"icon\" href=\"/logo.svg\">\n<style>{STYLE}</style>\n</head>\n\
         <body>\n{body}\n</body>\n</html>\n"
    ))
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn message(title: &str, text: &str) -> Html<String> {
    let body = format!(
        "<main class=\"narrow\"><a class=\"brand\" href=\"/\"><img src=\"/logo.svg\" alt=\"\">Motile</a>\
         <h1>{}</h1><p>{}</p></main>",
        escape(title),
        escape(text)
    );
    page(&format!("{title} · Motile"), &body)
}

pub async fn home() -> Html<String> {
    page("Motile · A command center for your coding agents", HOME)
}

pub async fn privacy() -> Html<String> {
    page("Privacy · Motile", PRIVACY)
}

pub async fn terms() -> Html<String> {
    page("Terms · Motile", TERMS)
}

pub async fn logo() -> Response {
    ([(CONTENT_TYPE, "image/svg+xml"), (CACHE_CONTROL, "public, max-age=86400")], LOGO).into_response()
}

pub async fn install_script(State(state): State<AppState>) -> Response {
    let script = INSTALL_SCRIPT.replace("__PUBLIC_URL__", &state.config.public_url);
    ([(CONTENT_TYPE, "text/x-shellscript; charset=utf-8"), (CACHE_CONTROL, "no-cache")], script).into_response()
}

fn is_release_file(name: &str) -> bool {
    let allowed = |character: char| character.is_ascii_alphanumeric() || "-_.".contains(character);
    !name.is_empty() && name.len() <= 100 && !name.starts_with('.') && name.chars().all(allowed)
}

pub async fn download(State(state): State<AppState>, Path(file): Path<String>) -> Response {
    if !is_release_file(&file) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(folder) = &state.config.download_dir else {
        return Redirect::temporary(&format!("{}/{file}", state.config.releases_url)).into_response();
    };
    match tokio::fs::read(std::path::Path::new(folder).join(&file)).await {
        Ok(bytes) => ([(CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, message("Not found", "There is nothing at this address.")).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_file_names_are_downloads() {
        assert!(is_release_file("motile-x86_64-unknown-linux-musl.tar.gz"));
        assert!(is_release_file("Motile.zip"));
        assert!(!is_release_file("../secrets"));
        assert!(!is_release_file(".env"));
        assert!(!is_release_file("a/b"));
    }
}
