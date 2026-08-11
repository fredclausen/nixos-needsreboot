//! Deciding whether an activated generation requires a reboot.

use crate::compare_nixos_modules::{self, CompareError};
use std::path::Path;

/// The outcome of comparing the booted closure with the activated closure.
#[derive(Debug, PartialEq, Eq)]
pub enum RebootDecision {
    /// The booted closure *is* the activated closure.
    SameGeneration,
    /// A new generation is activated, but neither the kernel nor systemd moved.
    NoRebootWorthyChange,
    /// A reboot is required; one reason per module that got newer.
    Needed(Vec<String>),
}

impl RebootDecision {
    /// Whether this decision requires the machine to be rebooted.
    #[must_use]
    pub const fn requires_reboot(&self) -> bool {
        matches!(self, Self::Needed(_))
    }
}

/// Decide whether the activated generation requires a reboot.
///
/// Generations are identified by their store closure path. `nixos-version`
/// must not be used for this: it holds `config.system.nixos.label`, which on a
/// flake that does not embed a revision in the label (e.g. a bare
/// `25.11pre-git`) is byte-identical for every generation ever built, so an
/// identity check based on it always reports `SameGeneration` and the kernel is
/// never compared.
///
/// # Errors
///
/// Returns an error if either closure cannot be inspected. A failed comparison
/// is never downgraded to "no reboot needed".
pub fn decide(old_system: &Path, new_system: &Path) -> Result<RebootDecision, CompareError> {
    if old_system == new_system {
        return Ok(RebootDecision::SameGeneration);
    }

    debug!(
        "Booted generation {} differs from activated generation {}",
        old_system.display(),
        new_system.display()
    );

    let reason = compare_nixos_modules::upgrades_available(old_system, new_system)?;

    if reason.is_empty() {
        return Ok(RebootDecision::NoRebootWorthyChange);
    }

    Ok(RebootDecision::Needed(reason))
}

#[cfg(test)]
mod tests {
    use super::RebootDecision;

    #[test]
    fn only_a_needed_decision_requires_a_reboot() {
        assert!(!RebootDecision::SameGeneration.requires_reboot());
        assert!(!RebootDecision::NoRebootWorthyChange.requires_reboot());
        assert!(
            RebootDecision::Needed(vec!["Linux Kernel (1 -> 2)\n".to_string()]).requires_reboot()
        );
    }
}
