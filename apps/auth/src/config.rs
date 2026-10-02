use std::env;

use motile_protocol::DEFAULT_AUTH_URL;

const DEFAULT_INSTALL_URL: &str = "https://motile.app/install.sh";

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
    /// The installer the install command runs (`apps/marketing/public/install.sh`).
    pub install_url: String,
    /// The web app's address. Sign-ins it starts end at its `/auth/callback`; without it there
    /// are none.
    pub web_url: Option<String>,
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
            install_url: optional("INSTALL_URL").unwrap_or_else(|| DEFAULT_INSTALL_URL.to_string()),
            web_url: optional("WEB_URL").map(|url| url.trim_end_matches('/').to_string()),
        })
    }

    /// Where the web app is handed the code of a sign-in it started.
    pub fn web_redirect(&self) -> Option<String> {
        self.web_url.as_ref().map(|url| format!("{url}/auth/callback"))
    }

    /// The installer links a server with Motile's own auth server unless it is told another.
    pub fn install_command(&self, token: &str) -> String {
        if self.public_url == DEFAULT_AUTH_URL {
            return format!("curl -fsSL {} | sh -s -- {token}", self.install_url);
        }
        format!("curl -fsSL {} | MOTILE_AUTH_URL={} sh -s -- {token}", self.install_url, self.public_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(public_url: &str) -> Config {
        Config {
            public_url: public_url.into(),
            database_url: String::new(),
            port: 0,
            google_client_id: String::new(),
            google_client_secret: String::new(),
            google_authorize_url: String::new(),
            google_token_url: String::new(),
            dev_login: false,
            install_url: DEFAULT_INSTALL_URL.into(),
            web_url: None,
        }
    }

    #[test]
    fn the_install_command_names_the_auth_server_only_when_it_is_not_motiles() {
        assert_eq!(
            config("https://auth.motile.app").install_command("token"),
            "curl -fsSL https://motile.app/install.sh | sh -s -- token"
        );
        assert_eq!(
            config("https://auth.example.com").install_command("token"),
            "curl -fsSL https://motile.app/install.sh | MOTILE_AUTH_URL=https://auth.example.com sh -s -- token"
        );
    }
}
