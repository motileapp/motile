use std::env;

const DEFAULT_RELEASES_URL: &str = "https://github.com/motileapp/motile/releases/latest/download";

pub struct Config {
    pub public_url: String,
    pub database_url: String,
    pub port: u16,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub google_authorize_url: String,
    pub google_token_url: String,
    /// Lets anyone sign in as any address without Google. For tests and local work only.
    pub dev_login: bool,
    /// Where `/download/<file>` sends people.
    pub releases_url: String,
    /// When set, `/download/<file>` serves files from this folder instead.
    pub download_dir: Option<String>,
    /// The web app's address. Sign-ins it starts end at its `/auth/callback`; without it there
    /// are none.
    pub web_url: Option<String>,
    /// The built marketing site (`apps/site/dist`), served for every address that isn't a route.
    pub site_dir: Option<String>,
}

fn required(name: &str) -> Result<String, String> {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        _ => Err(format!("{name} is not set")),
    }
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let port = match env::var("PORT") {
            Ok(port) => port.parse().map_err(|_| "PORT is not a number")?,
            Err(_) => 3000,
        };
        Ok(Self {
            public_url: required("PUBLIC_URL")?.trim_end_matches('/').to_string(),
            database_url: required("DATABASE_URL")?,
            port,
            google_client_id: required("GOOGLE_CLIENT_ID")?,
            google_client_secret: required("GOOGLE_CLIENT_SECRET")?,
            google_authorize_url: crate::google::AUTHORIZE_URL.to_string(),
            google_token_url: crate::google::TOKEN_URL.to_string(),
            dev_login: optional("DEV_LOGIN").is_some_and(|value| value == "1"),
            releases_url: optional("RELEASES_URL").unwrap_or_else(|| DEFAULT_RELEASES_URL.to_string()),
            download_dir: optional("DOWNLOAD_DIR"),
            web_url: optional("WEB_URL").map(|url| url.trim_end_matches('/').to_string()),
            site_dir: optional("SITE_DIR"),
        })
    }

    /// Where the web app is handed the code of a sign-in it started.
    pub fn web_redirect(&self) -> Option<String> {
        self.web_url.as_ref().map(|url| format!("{url}/auth/callback"))
    }

    pub fn install_command(&self, token: &str) -> String {
        format!("curl -fsSL {}/install | sh -s -- {token}", self.public_url)
    }
}
