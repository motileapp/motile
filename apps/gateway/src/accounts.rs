//! An account: one Claude organisation or one ChatGPT account the gateway holds the tokens of, with what
//! the user set for it and what its limits read last.

use motile_protocol::gateway_wire::AccountStatus;
use motile_protocol::wire::{Agent, LimitReset, LimitWindow};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Account {
    /// Claude's organisation, or Codex's account: never the email.
    pub id: String,
    pub agent: Agent,
    pub name: String,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub priority: u32,
    pub enabled: bool,
    pub status: AccountStatus,
    pub tokens: Tokens,
    pub windows: Vec<LimitWindow>,
    pub read_at: Option<f64>,
    pub resets: Vec<LimitReset>,
    pub created_at: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires_at: f64,
    /// Codex's ChatGPT account, which its backend is told with every request.
    pub chatgpt_account: Option<String>,
}

impl Account {
    /// It can take a request now.
    pub fn available(&self, now: f64) -> bool {
        self.enabled && self.status(now) == AccountStatus::Ready
    }

    /// What it is now: a rest that ended is over.
    pub fn status(&self, now: f64) -> AccountStatus {
        match &self.status {
            AccountStatus::Resting { until } if until.is_none_or(|until| until <= now) => AccountStatus::Ready,
            status => status.clone(),
        }
    }

    /// How many times its resets can still be used now.
    pub fn usable_resets(&self) -> u32 {
        self.resets.iter().filter(|reset| reset.usable).map(|reset| reset.left).sum()
    }

    /// Takes in what a response or the agent's usage page said of its windows.
    pub fn read(&mut self, reading: &crate::limits::Reading, now: f64) {
        if reading.whole {
            self.windows = reading.windows.clone();
        } else {
            for window in &reading.windows {
                match self.windows.iter_mut().find(|kept| kept.label == window.label) {
                    Some(kept) => *kept = window.clone(),
                    None => self.windows.push(window.clone()),
                }
            }
        }
        if let Some(resets) = &reading.resets {
            self.resets = resets.clone();
        }
        self.read_at = Some(now);
        // Refilled before its reset, as by a reset used elsewhere: it no longer rests.
        let room =
            reading.whole && !self.windows.is_empty() && self.windows.iter().all(|window| window.used_percent < 100.0);
        if room && matches!(self.status, AccountStatus::Resting { .. }) {
            self.status = AccountStatus::Ready;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::Reading;

    fn window(label: &str, used_percent: f64) -> LimitWindow {
        LimitWindow { label: label.into(), used_percent, resets_at: None, window_secs: None, warning: false }
    }

    #[test]
    fn a_responses_headers_change_only_the_windows_they_carry() {
        let mut account = Account {
            id: "org".into(),
            agent: Agent::Claude,
            name: "Work".into(),
            email: None,
            plan: None,
            priority: 0,
            enabled: true,
            status: AccountStatus::Resting { until: Some(50.0) },
            tokens: Tokens::default(),
            windows: vec![window("Session", 10.0), window("Weekly · Opus", 30.0)],
            read_at: None,
            resets: vec![],
            created_at: 0.0,
        };
        assert!(!account.available(40.0) && account.available(60.0));

        account.read(&Reading { windows: vec![window("Session", 20.0)], resets: None, whole: false }, 70.0);
        let used: Vec<_> = account.windows.iter().map(|window| window.used_percent).collect();
        assert_eq!((used, account.read_at), (vec![20.0, 30.0], Some(70.0)));

        let resets = motile_protocol::resets::codex(&serde_json::json!({"available_count": 1}));
        account
            .read(&Reading { windows: vec![window("Weekly", 5.0)], resets: Some(resets.clone()), whole: true }, 80.0);
        assert_eq!((account.windows.len(), account.resets.clone()), (1, resets));
    }

    #[test]
    fn an_account_refilled_before_its_reset_no_longer_rests() {
        let resting = AccountStatus::Resting { until: Some(500.0) };
        let mut account = Account {
            id: "org".into(),
            agent: Agent::Claude,
            name: "Work".into(),
            email: None,
            plan: None,
            priority: 0,
            enabled: true,
            status: resting.clone(),
            tokens: Tokens::default(),
            windows: vec![window("Session", 100.0)],
            read_at: None,
            resets: vec![],
            created_at: 0.0,
        };
        account.read(&Reading { windows: vec![window("Session", 40.0)], resets: None, whole: false }, 10.0);
        assert_eq!(account.status, resting, "a response's headers aren't all of its windows");
        account.read(&Reading { windows: vec![window("Session", 100.0)], resets: None, whole: true }, 20.0);
        assert_eq!(account.status, resting, "still used up");
        account.read(&Reading { windows: vec![window("Session", 0.0)], resets: None, whole: true }, 30.0);
        assert_eq!(account.status, AccountStatus::Ready);
    }
}
