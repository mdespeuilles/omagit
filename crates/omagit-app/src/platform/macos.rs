//! macOS — the distribution target.

use std::path::PathBuf;

use super::{Platform, PrimaryModifier, TopbarReserve};

pub struct MacOs;

/// The system draws the traffic lights over the topbar. The first 78 pixels are
/// off limits — no control ever enters them (DESIGN-TOKENS §9, SPEC §9).
const TRAFFIC_LIGHT_RESERVE: f32 = 78.0;

impl Platform for MacOs {
    fn name(&self) -> &'static str {
        "macos"
    }

    fn config_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME")
            .filter(|home| !home.is_empty())
            .map(|home| PathBuf::from(home).join("Library/Application Support/omagit"))
    }

    fn primary_modifier(&self) -> PrimaryModifier {
        PrimaryModifier::Command
    }

    fn topbar_reserve(&self) -> TopbarReserve {
        TopbarReserve {
            leading: TRAFFIC_LIGHT_RESERVE,
            trailing: 0.0,
        }
    }

    fn credential_helper(&self) -> &'static str {
        "osxkeychain"
    }

    fn omarchy_state_dir(&self) -> Option<PathBuf> {
        // There is no Omarchy on macOS. Embedded themes are the default
        // experience here, not a degraded fallback (SPEC §6.1).
        None
    }

    fn reports_system_appearance(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn the_traffic_light_band_is_reserved_on_the_leading_edge() {
        let reserve = MacOs.topbar_reserve();
        assert_eq!(reserve.leading, 78.0);
        assert_eq!(reserve.trailing, 0.0);
    }
}
