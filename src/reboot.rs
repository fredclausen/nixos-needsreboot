//! Deciding whether an activated generation requires a reboot.

use crate::compare_nixos_modules::{self, CompareError};
use crate::generations::Generations;
use std::{fmt, path::Path};

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

    /// Fold a staged generation into this decision. A staged generation is
    /// reboot-worthy on its own, so it turns any decision into
    /// [`RebootDecision::Needed`] and is listed after the module upgrades.
    #[must_use]
    pub fn including(self, staged: Option<StagedGeneration>) -> Self {
        let Some(staged) = staged else {
            return self;
        };

        let mut reasons = match self {
            Self::Needed(reasons) => reasons,
            Self::SameGeneration | Self::NoRebootWorthyChange => vec![],
        };
        reasons.push(format!("{staged}\n"));

        Self::Needed(reasons)
    }
}

/// A registered generation, other than the running one, that the bootloader
/// will start next, as left behind by `nixos-rebuild boot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagedGeneration {
    /// The generation number of the running system.
    pub running: u64,
    /// The generation number the next boot will start.
    pub next_boot: u64,
}

// The flag file and `--dry-run` output carry this text.
impl fmt::Display for StagedGeneration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Staged generation (running {} -> next boot {})",
            self.running, self.next_boot
        )
    }
}

/// Detect a generation staged for the next boot.
///
/// That is the case when the running closure differs from the system profile
/// *and* the running closure is itself a registered generation. The second
/// condition is what separates `nixos-rebuild boot` (the profile moved on
/// while the running system stayed put) from `nixos-rebuild test` (a
/// temporary closure was activated without ever entering the profile), which
/// is deliberately not reported: rebooting would only throw the test away.
///
/// A staged generation needs a reboot even when its kernel and systemd are
/// unchanged, because none of its configuration is live until then.
#[must_use]
pub fn staged_generation(
    current: &Path,
    profile: &Path,
    generations: &Generations,
) -> Option<StagedGeneration> {
    if current == profile {
        return None;
    }

    let Some(running) = generations.number_of(current) else {
        debug!(
            "Running system {} is not a registered generation; nothing is staged",
            current.display()
        );
        return None;
    };

    let Some(next_boot) = generations.number_of(profile) else {
        debug!(
            "System profile {} is not a registered generation; nothing is staged",
            profile.display()
        );
        return None;
    };

    debug!("Generation {next_boot} is staged; generation {running} is running");

    Some(StagedGeneration { running, next_boot })
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
    use super::{RebootDecision, StagedGeneration};

    const STAGED: StagedGeneration = StagedGeneration {
        running: 41,
        next_boot: 42,
    };

    #[test]
    fn a_staged_generation_alone_requires_a_reboot() {
        for decision in [
            RebootDecision::SameGeneration,
            RebootDecision::NoRebootWorthyChange,
        ] {
            assert_eq!(
                decision.including(Some(STAGED)),
                RebootDecision::Needed(vec![
                    "Staged generation (running 41 -> next boot 42)\n".to_string()
                ])
            );
        }
    }

    #[test]
    fn a_staged_generation_is_listed_after_module_upgrades() {
        let decision = RebootDecision::Needed(vec!["Linux Kernel (1 -> 2)\n".to_string()]);

        assert_eq!(
            decision.including(Some(STAGED)),
            RebootDecision::Needed(vec![
                "Linux Kernel (1 -> 2)\n".to_string(),
                "Staged generation (running 41 -> next boot 42)\n".to_string()
            ])
        );
    }

    #[test]
    fn nothing_staged_leaves_the_decision_alone() {
        assert_eq!(
            RebootDecision::NoRebootWorthyChange.including(None),
            RebootDecision::NoRebootWorthyChange
        );
    }

    #[test]
    fn only_a_needed_decision_requires_a_reboot() {
        assert!(!RebootDecision::SameGeneration.requires_reboot());
        assert!(!RebootDecision::NoRebootWorthyChange.requires_reboot());
        assert!(
            RebootDecision::Needed(vec!["Linux Kernel (1 -> 2)\n".to_string()]).requires_reboot()
        );
    }
}
