//! What the settings say of a server's computer use.

use motile_protocol::wire::{ComputerUse, ComputerUseState};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ComputerUseView {
    pub enabled: bool,
    /// What it needs before the agents can use it, to show under the server's name.
    pub problem: Option<String>,
}

/// None for a server that can't give its agents its screen.
pub fn view(computer_use: Option<&ComputerUse>, server_name: &str) -> Option<ComputerUseView> {
    let computer_use = computer_use?;
    let problem = match &computer_use.state {
        ComputerUseState::NeedsPermissions { accessibility, screen_recording } => {
            let needed = match (accessibility, screen_recording) {
                (true, true) => "Accessibility and Screen Recording",
                (true, false) => "Accessibility",
                _ => "Screen Recording",
            };
            Some(format!("Allow motile under {needed} in System Settings → Privacy & Security on {server_name}"))
        }
        ComputerUseState::Failed { message } => Some(message.clone()),
        _ => None,
    };
    Some(ComputerUseView { enabled: computer_use.enabled, problem })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem(state: ComputerUseState) -> Option<String> {
        view(Some(&ComputerUse { enabled: true, state }), "ymini").unwrap().problem
    }

    #[test]
    fn a_server_that_lacks_permissions_says_which_to_grant_where() {
        let both = ComputerUseState::NeedsPermissions { accessibility: true, screen_recording: true };
        assert_eq!(
            problem(both).unwrap(),
            "Allow motile under Accessibility and Screen Recording in System Settings → Privacy & Security on ymini"
        );
        let one = ComputerUseState::NeedsPermissions { accessibility: false, screen_recording: true };
        assert!(problem(one).unwrap().starts_with("Allow motile under Screen Recording in"));
    }

    #[test]
    fn only_what_needs_fixing_is_said() {
        let failed = ComputerUseState::Failed { message: "No display is free.".into() };
        assert_eq!(problem(failed).as_deref(), Some("No display is free."));
        for state in
            [ComputerUseState::Off, ComputerUseState::Starting, ComputerUseState::Ready { virtual_screen: true }]
        {
            assert_eq!(problem(state), None);
        }
        assert_eq!(view(None, "old"), None);
    }
}
