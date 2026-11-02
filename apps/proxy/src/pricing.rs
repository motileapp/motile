//! What a response cost at the API's prices, from the list LiteLLM keeps, fetched once a day and
//! kept on the volume. Spending is measured in these dollars to compare accounts and threads.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::usage::Spent;

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
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
pub struct Prices(HashMap<String, Rates>);

impl Prices {
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
            };
            Some((model.to_lowercase(), rates))
        };
        Self(list.as_object().into_iter().flatten().filter_map(rates).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The model's rates, or those of the listed model whose name shares the most with it, so that
    /// a model newer than the list costs what its family does.
    fn rates(&self, model: &str) -> Option<Rates> {
        let model = model.to_lowercase();
        let model = model.split('[').next().unwrap_or_default();
        if let Some(rates) = self.0.get(model) {
            return Some(*rates);
        }
        let shared = |name: &str| name.bytes().zip(model.bytes()).take_while(|(a, b)| a == b).count();
        let (name, rates) = self.0.iter().max_by_key(|(name, _)| (shared(name), std::cmp::Reverse(name.len())))?;
        (shared(name) >= model.find('-').map_or(model.len(), |dash| dash + 1)).then_some(*rates)
    }

    pub fn cost(&self, spent: &Spent) -> f64 {
        let Some(rates) = spent.model.as_deref().and_then(|model| self.rates(model)) else { return 0.0 };
        spent.input as f64 * rates.input
            + spent.output as f64 * rates.output
            + spent.cache_read as f64 * rates.cache_read
            + spent.cache_write as f64 * rates.cache_write
    }
}

pub async fn fetch(http: &reqwest::Client, url: &str) -> anyhow::Result<Prices> {
    let response = http.get(url).timeout(FETCH_TIMEOUT).send().await.context("The price list couldn't be reached.")?;
    if !response.status().is_success() {
        bail!("The price list couldn't be read: {}.", response.status());
    }
    let prices = Prices::from_list(&response.json().await?);
    if prices.is_empty() {
        bail!("The price list names no model.");
    }
    Ok(prices)
}

pub fn read_kept(path: &Path) -> Option<Prices> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub fn keep(path: &Path, prices: &Prices) {
    if let Ok(text) = serde_json::to_string(prices) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn prices() -> Prices {
        Prices::from_list(&json!({
            "claude-opus-4-5": {"litellm_provider": "anthropic", "input_cost_per_token": 5e-6,
                "output_cost_per_token": 2.5e-5, "cache_read_input_token_cost": 5e-7,
                "cache_creation_input_token_cost": 6.25e-6},
            "gpt-5": {"litellm_provider": "openai", "input_cost_per_token": 1.25e-6, "output_cost_per_token": 1e-5,
                "cache_read_input_token_cost": 1.25e-7},
            "azure/gpt-5": {"litellm_provider": "azure", "input_cost_per_token": 9.0, "output_cost_per_token": 9.0},
        }))
    }

    fn spent(model: &str) -> Spent {
        Spent { model: Some(model.into()), input: 1_000_000, output: 100_000, cache_read: 1_000_000, cache_write: 0 }
    }

    #[test]
    fn a_response_costs_its_tokens_at_their_kinds_prices() {
        assert!((prices().cost(&spent("claude-opus-4-5")) - (5.0 + 2.5 + 0.5)).abs() < 1e-9);
        assert!((prices().cost(&spent("gpt-5")) - (1.25 + 1.0 + 0.125)).abs() < 1e-9);
    }

    #[test]
    fn a_model_newer_than_the_list_costs_what_its_family_does() {
        assert_eq!(prices().cost(&spent("claude-opus-4-6[1m]")), prices().cost(&spent("claude-opus-4-5")));
        assert_eq!(prices().cost(&spent("gpt-5.5-codex")), prices().cost(&spent("gpt-5")));
        assert_eq!(prices().cost(&spent("llama-3")), 0.0);
    }
}
