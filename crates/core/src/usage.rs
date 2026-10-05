//! What the agents spent on the account's servers, added up for the usage view: the totals, a
//! series for each agent's chart, and what each model and project took.

use std::collections::HashMap;

use motile_protocol::wire::{Agent, Tokens, UsageBucket};
use serde::Serialize;

const REMOVED_PROJECT: &str = "Removed project";
const UNKNOWN_MODEL: &str = "Unknown model";

/// The stretch of time the view shows: `buckets` spans of `bucket_secs`, the last of which holds
/// `now`, starting where a clock `utc_offset_secs` ahead of UTC starts its hours and days.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub since: f64,
    pub until: f64,
    pub bucket_secs: u32,
    pub utc_offset_secs: i32,
}

impl Window {
    pub fn ending(now: f64, bucket_secs: u32, buckets: u32, utc_offset_secs: i32) -> Self {
        let (size, offset) = (f64::from(bucket_secs.max(60)), f64::from(utc_offset_secs));
        let last = ((now + offset) / size).floor() * size - offset;
        let since = last - size * f64::from(buckets.saturating_sub(1));
        Self { since, until: last + size, bucket_secs: bucket_secs.max(60), utc_offset_secs }
    }

    fn starts(&self) -> Vec<f64> {
        let size = f64::from(self.bucket_secs);
        let count = ((self.until - self.since) / size).round() as usize;
        (0..count).map(|index| self.since + size * index as f64).collect()
    }
}

/// What a server said was spent in one of its projects.
pub struct Spent {
    pub server_id: String,
    pub bucket: UsageBucket,
}

#[derive(Serialize, Debug, PartialEq, Default)]
pub struct View {
    /// What the API would have charged for the tokens whose model has a known price.
    pub cost_usd: f64,
    pub tokens: u64,
    /// The tokens of models without a known price, which `cost_usd` leaves out.
    pub unpriced_tokens: u64,
    pub cache_savings_usd: f64,
    /// The part of the cost and of the tokens that went into writing titles, branch names,
    /// commit messages and pull requests.
    pub writing_cost_usd: f64,
    pub writing_tokens: u64,
    /// When each point of the agents' series starts.
    pub starts: Vec<f64>,
    pub agents: Vec<AgentSeries>,
    /// The kinds of token, with what each cost.
    pub kinds: Vec<Kind>,
    pub models: Vec<Line>,
    pub projects: Vec<Line>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct AgentSeries {
    pub agent: Agent,
    pub cost_usd: f64,
    pub tokens: u64,
    /// A point for each of the view's `starts`.
    pub cost_points: Vec<f64>,
    pub token_points: Vec<u64>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Kind {
    pub name: &'static str,
    pub tokens: u64,
    pub cost_usd: f64,
}

/// A model or a project and its part of the whole.
#[derive(Serialize, Debug, PartialEq)]
pub struct Line {
    pub name: String,
    pub agent: Option<Agent>,
    pub tokens: u64,
    /// Missing when none of it has a known price.
    pub cost_usd: Option<f64>,
    /// Its part of the cost, from 0 to 1, or of the tokens when nothing has a price.
    pub share: f64,
}

fn total(tokens: &Tokens) -> u64 {
    tokens.input + tokens.cache_read + tokens.cache_write + tokens.output
}

/// `project_names` are the projects' names by server and project.
pub fn view(spent: &[Spent], window: Window, project_names: &HashMap<(String, String), String>) -> View {
    let starts = window.starts();
    let point = |start: f64| {
        let index = ((start - window.since) / f64::from(window.bucket_secs)).round();
        (index >= 0.0 && (index as usize) < starts.len()).then_some(index as usize)
    };
    let mut view = View::default();
    let mut agents: Vec<AgentSeries> = Vec::new();
    let mut kinds = [("Input", 0, 0.0), ("Cache read", 0, 0.0), ("Cache write", 0, 0.0), ("Output", 0, 0.0)];
    let mut other_cost = 0.0;
    let mut models: Vec<Line> = Vec::new();
    let mut projects: Vec<Line> = Vec::new();
    let add = |lines: &mut Vec<Line>, name: &str, agent: Option<Agent>, tokens: u64, cost_usd: Option<f64>| {
        let found = lines.iter().position(|line| line.name == name && line.agent == agent);
        let index = found.unwrap_or_else(|| {
            lines.push(Line { name: name.to_string(), agent, tokens: 0, cost_usd: None, share: 0.0 });
            lines.len() - 1
        });
        let line = &mut lines[index];
        line.tokens += tokens;
        line.cost_usd = cost_usd.map(|cost| cost + line.cost_usd.unwrap_or_default()).or(line.cost_usd);
    };

    for Spent { server_id, bucket } in spent {
        let tokens = total(&bucket.tokens);
        let cost = bucket.cost_usd.unwrap_or_default();
        view.tokens += tokens;
        view.cost_usd += cost;
        view.cache_savings_usd += bucket.cache_savings_usd;
        if bucket.cost_usd.is_none() {
            view.unpriced_tokens += tokens;
        }
        if bucket.writing {
            view.writing_cost_usd += cost;
            view.writing_tokens += tokens;
        }

        let found = agents.iter().position(|series| series.agent == bucket.agent);
        let index = found.unwrap_or_else(|| {
            let (cost_points, token_points) = (vec![0.0; starts.len()], vec![0; starts.len()]);
            agents.push(AgentSeries { agent: bucket.agent, cost_usd: 0.0, tokens: 0, cost_points, token_points });
            agents.len() - 1
        });
        let series = &mut agents[index];
        series.cost_usd += cost;
        series.tokens += tokens;
        if let Some(point) = point(bucket.start) {
            series.cost_points[point] += cost;
            series.token_points[point] += tokens;
        }

        let counts = [bucket.tokens.input, bucket.tokens.cache_read, bucket.tokens.cache_write, bucket.tokens.output];
        let costs = bucket.costs.map(|costs| [costs.input, costs.cache_read, costs.cache_write, costs.output]);
        for (index, kind) in kinds.iter_mut().enumerate() {
            kind.1 += counts[index];
            kind.2 += costs.map(|costs| costs[index]).unwrap_or_default();
        }
        if costs.is_none() {
            other_cost += cost;
        }

        let model = if bucket.model.is_empty() { UNKNOWN_MODEL } else { &bucket.model };
        add(&mut models, model, Some(bucket.agent), tokens, bucket.cost_usd);
        let project = project_names.get(&(server_id.clone(), bucket.project_id.clone()));
        add(&mut projects, project.map(String::as_str).unwrap_or(REMOVED_PROJECT), None, tokens, bucket.cost_usd);
    }

    for lines in [&mut models, &mut projects] {
        for line in lines.iter_mut() {
            line.share = match view.cost_usd > 0.0 {
                true => line.cost_usd.unwrap_or_default() / view.cost_usd,
                false => line.tokens as f64 / view.tokens.max(1) as f64,
            };
        }
        lines.sort_by(|a, b| b.share.total_cmp(&a.share).then(b.tokens.cmp(&a.tokens)));
    }
    agents.sort_by_key(|series| series.agent != Agent::Claude);
    view.kinds = kinds.into_iter().map(|(name, tokens, cost_usd)| Kind { name, tokens, cost_usd }).collect();
    if other_cost > 0.0 {
        view.kinds.push(Kind { name: "Other", tokens: 0, cost_usd: other_cost });
    }
    View { starts, agents, models, projects, ..view }
}

#[cfg(test)]
mod tests {
    use motile_protocol::wire::TokenCosts;

    use super::*;

    const HOUR: f64 = 3600.0;
    const DAY: f64 = 86400.0;

    fn spent(server: &str, project: &str, agent: Agent, model: &str, start: f64, cost_usd: Option<f64>) -> Spent {
        let costs = cost_usd.map(|cost| TokenCosts { input: cost / 2.0, output: cost / 2.0, ..TokenCosts::default() });
        let bucket = UsageBucket {
            start,
            agent,
            model: model.to_string(),
            project_id: project.to_string(),
            tokens: Tokens { input: 100, cache_read: 800, cache_write: 0, output: 100 },
            cost_usd,
            costs,
            cache_savings_usd: 0.25,
            writing: false,
        };
        Spent { server_id: server.to_string(), bucket }
    }

    #[test]
    fn the_window_ends_with_the_day_that_holds_now_on_the_clients_clock() {
        // Half past one in the morning two hours ahead of UTC is still yesterday in UTC.
        let now = 10.0 * DAY - HOUR / 2.0;
        let window = Window::ending(now, 86400, 3, 2 * 3600);
        assert_eq!((window.since, window.until), (8.0 * DAY - 2.0 * HOUR, 11.0 * DAY - 2.0 * HOUR));
        assert_eq!(window.starts(), vec![8.0 * DAY - 2.0 * HOUR, 9.0 * DAY - 2.0 * HOUR, 10.0 * DAY - 2.0 * HOUR]);
    }

    #[test]
    fn what_was_spent_is_added_up_by_agent_model_and_project() {
        let window = Window { since: 0.0, until: 3.0 * DAY, bucket_secs: 86400, utc_offset_secs: 0 };
        let mut all = [
            spent("studio", "api", Agent::Codex, "gpt-6", DAY, Some(1.0)),
            spent("studio", "api", Agent::Claude, "claude-opus", 0.0, Some(2.0)),
            spent("studio", "web", Agent::Claude, "claude-opus", 2.0 * DAY, Some(1.0)),
            spent("attic", "api", Agent::Codex, "gpt-fake", DAY, None),
        ];
        all[0].bucket.writing = true;
        let name = |server: &str, project: &str, name: &str| ((server.to_string(), project.to_string()), name.into());
        let names = HashMap::from([name("studio", "api", "API"), name("studio", "web", "Web")]);

        let view = view(&all, window, &names);

        assert_eq!((view.cost_usd, view.tokens, view.unpriced_tokens, view.cache_savings_usd), (4.0, 4000, 1000, 1.0));
        assert_eq!((view.writing_cost_usd, view.writing_tokens), (1.0, 1000));
        assert_eq!(view.starts, vec![0.0, DAY, 2.0 * DAY]);
        let series: Vec<_> = view.agents.iter().map(|series| (series.agent, &series.cost_points[..])).collect();
        assert_eq!(series, vec![(Agent::Claude, &[2.0, 0.0, 1.0][..]), (Agent::Codex, &[0.0, 1.0, 0.0][..])]);
        assert_eq!(view.agents[1].token_points, vec![0, 2000, 0]);
        let kinds: Vec<_> = view.kinds.iter().map(|kind| (kind.name, kind.tokens, kind.cost_usd)).collect();
        let expected = [("Input", 400, 2.0), ("Cache read", 3200, 0.0), ("Cache write", 0, 0.0), ("Output", 400, 2.0)];
        assert_eq!(kinds, expected);
        let lines = |lines: &[Line]| -> Vec<(String, Option<f64>, f64)> {
            lines.iter().map(|line| (line.name.clone(), line.cost_usd, line.share)).collect()
        };
        let models = [("claude-opus", Some(3.0), 0.75), ("gpt-6", Some(1.0), 0.25), ("gpt-fake", None, 0.0)];
        assert_eq!(lines(&view.models), models.map(|(name, cost, share)| (name.to_string(), cost, share)));
        let projects = [("API", Some(3.0), 0.75), ("Web", Some(1.0), 0.25), ("Removed project", None, 0.0)];
        assert_eq!(lines(&view.projects), projects.map(|(name, cost, share)| (name.to_string(), cost, share)));
    }
}
