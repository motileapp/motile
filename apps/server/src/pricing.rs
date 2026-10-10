//! What the models cost at the API's prices, from the list LiteLLM keeps, to say what the tokens
//! an agent spent would have been charged.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, bail};
use motile_protocol::wire::{TokenCosts, Tokens, UsageBucket};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LIST_URL: &str =
    "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";
const PROVIDERS: [&str; 2] = ["anthropic", "openai"];
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Dollars a token.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Rates {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
    /// How many tokens the model reads at most.
    #[serde(default)]
    pub context_window: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Prices(HashMap<String, Rates>);

impl Prices {
    /// The models of Anthropic and OpenAI in LiteLLM's list.
    pub fn from_list(list: &Value) -> Self {
        let rates = |(model, entry): (&String, &Value)| {
            if !PROVIDERS.contains(&entry["litellm_provider"].as_str()?) {
                return None;
            }
            let input = entry["input_cost_per_token"].as_f64()?;
            let rates = Rates {
                input,
                cache_read: entry["cache_read_input_token_cost"].as_f64().unwrap_or(input),
                cache_write: entry["cache_creation_input_token_cost"].as_f64().unwrap_or(input),
                output: entry["output_cost_per_token"].as_f64()?,
                context_window: entry["max_input_tokens"].as_u64(),
            };
            Some((model.to_lowercase(), rates))
        };
        Self(list.as_object().into_iter().flatten().filter_map(rates).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn rates(&self, model: &str) -> Option<Rates> {
        let model = model.to_lowercase();
        // Claude Code names a model with a larger context as `claude-…[1m]`.
        let model = model.split('[').next().unwrap_or_default();
        self.0.get(model).copied()
    }

    /// How many tokens the model reads at most, as the list says.
    pub fn context_window(&self, model: &str) -> Option<u64> {
        if model.to_lowercase().ends_with("[1m]") {
            return Some(1_000_000);
        }
        self.rates(model)?.context_window
    }

    /// Fills in what the bucket's tokens cost. What the agent said they cost stands, and is
    /// divided among the kinds of token as the list's prices divide it.
    pub fn price(&self, bucket: &mut UsageBucket) {
        let Some(rates) = self.rates(&bucket.model) else { return };
        let Tokens { input, cache_read, cache_write, output } = bucket.tokens;
        let listed = TokenCosts {
            input: input as f64 * rates.input,
            cache_read: cache_read as f64 * rates.cache_read,
            cache_write: cache_write as f64 * rates.cache_write,
            output: output as f64 * rates.output,
        };
        let listed_total = listed.input + listed.cache_read + listed.cache_write + listed.output;
        let cost = bucket.cost_usd.unwrap_or(listed_total);
        let scale = if listed_total > 0.0 { cost / listed_total } else { 1.0 };
        bucket.cost_usd = Some(cost);
        bucket.costs = Some(TokenCosts {
            input: listed.input * scale,
            cache_read: listed.cache_read * scale,
            cache_write: listed.cache_write * scale,
            output: listed.output * scale,
        });
        bucket.cache_savings_usd = cache_read as f64 * (rates.input - rates.cache_read).max(0.0);
    }
}

pub async fn fetch(url: &str) -> anyhow::Result<Prices> {
    motile_protocol::tls::install();
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build()?;
    let response = client.get(url).send().await.context("The price list couldn't be reached.")?;
    if !response.status().is_success() {
        bail!("The price list couldn't be read: {}.", response.status());
    }
    let prices = Prices::from_list(&response.json().await?);
    if prices.is_empty() {
        bail!("The price list names no model.");
    }
    Ok(prices)
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::Agent;
    use serde_json::json;

    use super::*;

    fn bucket(model: &str, tokens: Tokens, cost_usd: Option<f64>) -> UsageBucket {
        UsageBucket {
            start: 0.0,
            agent: Agent::Codex,
            account_name: String::new(),
            model: model.to_string(),
            project_id: "p".to_string(),
            tokens,
            cost_usd,
            costs: None,
            cache_savings_usd: 0.0,
            writing: false,
        }
    }

    fn prices() -> Prices {
        Prices::from_list(&json!({
            "sample_spec": {"litellm_provider": "one of the providers"},
            "gpt-6": {"litellm_provider": "openai", "input_cost_per_token": 2e-6, "output_cost_per_token": 1e-5,
                      "cache_read_input_token_cost": 2e-7},
            "azure/gpt-6": {"litellm_provider": "azure", "input_cost_per_token": 3e-6, "output_cost_per_token": 1e-5},
            "claude-haiku-4-5": {"litellm_provider": "anthropic", "input_cost_per_token": 1e-6,
                "output_cost_per_token": 5e-6, "cache_read_input_token_cost": 1e-7,
                "cache_creation_input_token_cost": 1.25e-6, "max_input_tokens": 200000},
        }))
    }

    #[test]
    fn tokens_are_priced_from_the_list_by_kind() {
        let tokens = Tokens { input: 1_000_000, cache_read: 1_000_000, cache_write: 0, output: 100_000 };
        let mut spent = bucket("GPT-6", tokens, None);
        prices().price(&mut spent);
        let costs = spent.costs.unwrap();
        let near = |cost: f64, expected: f64| (cost - expected).abs() < 1e-9;
        assert!(near(costs.input, 2.0) && near(costs.cache_read, 0.2) && near(costs.output, 1.0));
        assert_eq!(costs.cache_write, 0.0);
        assert!((spent.cost_usd.unwrap() - 3.2).abs() < 1e-9);
        assert!((spent.cache_savings_usd - 1.8).abs() < 1e-9, "reading it again would have cost 2.0");
    }

    #[test]
    fn what_the_agent_said_it_cost_stands_and_is_divided_as_the_list_does() {
        let tokens = Tokens { input: 0, cache_read: 0, cache_write: 1_000_000, output: 250_000 };
        let mut spent = bucket("claude-haiku-4-5[1m]", tokens, Some(5.0));
        prices().price(&mut spent);
        let costs = spent.costs.unwrap();
        assert_eq!(spent.cost_usd, Some(5.0));
        assert!((costs.cache_write - 2.5).abs() < 1e-9 && (costs.output - 2.5).abs() < 1e-9);
    }

    #[test]
    fn a_models_context_window_is_what_the_list_says_it_reads_at_most() {
        assert_eq!(prices().context_window("claude-haiku-4-5"), Some(200_000));
        assert_eq!(prices().context_window("claude-haiku-4-5[1m]"), Some(1_000_000));
        assert_eq!(prices().context_window("gpt-6"), None);
        assert_eq!(prices().context_window("gpt-fake"), None);
    }

    #[test]
    fn a_model_the_list_doesnt_name_keeps_what_is_known() {
        let mut unknown = bucket("gpt-fake", Tokens { input: 10, ..Tokens::default() }, None);
        let mut other_provider = bucket("azure/gpt-6", Tokens::default(), Some(0.5));
        prices().price(&mut unknown);
        prices().price(&mut other_provider);
        assert_eq!((unknown.cost_usd, unknown.costs), (None, None));
        assert_eq!((other_provider.cost_usd, other_provider.costs), (Some(0.5), None));
    }
}
