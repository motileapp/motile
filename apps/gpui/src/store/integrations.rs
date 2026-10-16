//! The issue trackers whose tabs the right panel offers once the settings turn them on, kept on
//! this device.

use super::Store;
use crate::panel::state::PanelTab;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Integration {
    Linear,
    Ricrac,
}

impl Integration {
    pub const ALL: [Integration; 2] = [Integration::Linear, Integration::Ricrac];

    pub fn id(self) -> &'static str {
        match self {
            Integration::Linear => "linear",
            Integration::Ricrac => "ricrac",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Integration::Linear => "Linear",
            Integration::Ricrac => "Ricrac",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Integration::Linear => "Browse, file and change Linear issues, and hand them to an agent.",
            Integration::Ricrac => "Browse, file and change Ricrac issues, and hand them to an agent.",
        }
    }

    /// Its logo, which is also its symbol.
    pub fn symbol(self) -> &'static str {
        self.id()
    }

    fn key(self) -> String {
        format!("integration.{}", self.id())
    }

    fn on_by_default(self) -> bool {
        self == Integration::Linear
    }
}

impl PanelTab {
    /// The integration the tab is of, which hides it while it is off.
    pub fn integration(&self) -> Option<Integration> {
        match self {
            PanelTab::Linear | PanelTab::LinearIssue { .. } => Some(Integration::Linear),
            PanelTab::Ricrac | PanelTab::RicracIssue { .. } => Some(Integration::Ricrac),
            _ => None,
        }
    }
}

impl Store {
    pub fn is_on(&self, integration: Integration) -> bool {
        self.prefs.get(&integration.key()).unwrap_or(integration.on_by_default())
    }

    pub fn set_integration(&mut self, integration: Integration, on: bool) {
        self.prefs.set(&integration.key(), on);
    }
}
