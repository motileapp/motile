//! Where the gateway reaches Anthropic and OpenAI. Each can be set from the environment, so that tests
//! and a local run can stand fakes in for them.

use crate::pricing;

#[derive(Clone, Debug)]
pub struct Upstreams {
    /// Messages, and Claude's profile and usage.
    pub anthropic: String,
    /// The page a Claude sign-in opens.
    pub claude_authorize: String,
    pub claude_token: String,
    /// The ChatGPT backend: Codex's responses under `/codex`, its usage under `/wham`.
    pub chatgpt: String,
    /// Codex's sign-in and its tokens.
    pub openai_auth: String,
    pub prices: String,
    /// Where an MCP server's sign-in comes back to: the auth server, which sends it on to
    /// `motile://mcp`.
    pub mcp_redirect: String,
}

impl Default for Upstreams {
    fn default() -> Self {
        Self {
            anthropic: "https://api.anthropic.com".into(),
            claude_authorize: "https://claude.com/cai/oauth/authorize".into(),
            claude_token: "https://platform.claude.com/v1/oauth/token".into(),
            chatgpt: "https://chatgpt.com/backend-api".into(),
            openai_auth: "https://auth.openai.com".into(),
            prices: pricing::LIST_URL.into(),
            mcp_redirect: mcp_redirect(motile_protocol::DEFAULT_AUTH_URL),
        }
    }
}

impl Upstreams {
    pub fn from_env() -> Self {
        let real = Self::default();
        let read = |name: &str, default: String| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or(default)
                .trim_end_matches('/')
                .to_string()
        };
        Self {
            anthropic: read("MOTILE_ANTHROPIC_URL", real.anthropic),
            claude_authorize: read("MOTILE_CLAUDE_AUTHORIZE_URL", real.claude_authorize),
            claude_token: read("MOTILE_CLAUDE_TOKEN_URL", real.claude_token),
            chatgpt: read("MOTILE_CHATGPT_URL", real.chatgpt),
            openai_auth: read("MOTILE_OPENAI_AUTH_URL", real.openai_auth),
            prices: read("MOTILE_PRICES_URL", real.prices),
            mcp_redirect: real.mcp_redirect,
        }
    }

    /// Where every upstream is `base`, as a test's fake serves them.
    pub fn at(base: &str) -> Self {
        Self {
            anthropic: base.into(),
            claude_authorize: format!("{base}/oauth/authorize"),
            claude_token: format!("{base}/v1/oauth/token"),
            chatgpt: format!("{base}/backend-api"),
            openai_auth: base.into(),
            prices: format!("{base}/prices.json"),
            mcp_redirect: mcp_redirect(base),
        }
    }
}

/// Where an MCP server's sign-in comes back to on the auth server at `auth_url`.
pub fn mcp_redirect(auth_url: &str) -> String {
    format!("{}/mcp/callback", auth_url.trim_end_matches('/'))
}
