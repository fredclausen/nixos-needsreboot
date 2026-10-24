//! The numbered generations registered in a NixOS system profile.
//!
//! `nixos-rebuild` records every generation it builds as a
//! `<profile>-<number>-link` symlink next to the profile itself, e.g.
//! `/nix/var/nix/profiles/system-42-link`. A closure that appears here was
//! installed into the profile; one that does not (such as the result of
//! `nixos-rebuild test`) was only ever activated temporarily.

use crate::error::Error;
use std::{fs, path::Path, path::PathBuf};

/// One registered generation of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    /// The generation number, `N` in `system-N-link`.
    pub number: u64,
    /// The store closure the generation link resolves to.
    pub closure: PathBuf,
}

/// Every registered generation of a profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generations(Vec<Generation>);

impl Generations {
    /// Build a set of generations directly, without reading a profile.
    #[must_use]
    pub const fn new(generations: Vec<Generation>) -> Self {
        Self(generations)
    }

    /// Read the generations registered next to `profile`.
    ///
    /// # Errors
    ///
    /// Returns an error if `profile` does not name a profile, if its
    /// directory cannot be listed, or if a generation link cannot be resolved.
    pub fn read(profile: &Path) -> Result<Self, Error> {
        let (Some(directory), Some(name)) = (
            profile.parent(),
            profile.file_name().and_then(|name| name.to_str()),
        ) else {
            return Err(Error::NotAProfile {
                path: profile.to_path_buf(),
            });
        };

        let entries = fs::read_dir(directory).map_err(|source| Error::GenerationsUnreadable {
            path: directory.to_path_buf(),
            source,
        })?;

        let mut generations = vec![];

        for entry in entries {
            let entry = entry.map_err(|source| Error::GenerationsUnreadable {
                path: directory.to_path_buf(),
                source,
            })?;

            let Some(number) = entry
                .file_name()
                .to_str()
                .and_then(|link| generation_number(name, link))
            else {
                continue;
            };

            let link = entry.path();
            let closure = fs::canonicalize(&link)
                .map_err(|source| Error::UnresolvableSystem { path: link, source })?;

            generations.push(Generation { number, closure });
        }

        debug!(
            "Found {} registered generations of {}",
            generations.len(),
            profile.display()
        );

        Ok(Self(generations))
    }

    /// The highest-numbered generation whose closure is `closure`, if any.
    ///
    /// Rebuilding an unchanged configuration registers a new generation with
    /// the same closure, so one closure can belong to several generations.
    #[must_use]
    pub fn number_of(&self, closure: &Path) -> Option<u64> {
        self.0
            .iter()
            .filter(|generation| generation.closure == closure)
            .map(|generation| generation.number)
            .max()
    }
}

/// Parse the generation number out of `<profile>-<number>-link`.
fn generation_number(profile: &str, link: &str) -> Option<u64> {
    link.strip_prefix(profile)?
        .strip_prefix('-')?
        .strip_suffix("-link")?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{generation_number, Generation, Generations};
    use std::path::{Path, PathBuf};

    #[test]
    fn a_generation_link_yields_its_number() {
        assert_eq!(generation_number("system", "system-42-link"), Some(42));
        assert_eq!(generation_number("system", "system-1-link"), Some(1));
    }

    /// The profile itself, other profiles, and anything not shaped like a
    /// generation link must be ignored.
    #[test]
    fn anything_else_in_the_profile_directory_is_not_a_generation() {
        for link in [
            "system",
            "system-link",
            "system--link",
            "system-42",
            "system-x-link",
            "system-profiles",
            "per-user",
            "default-3-link",
            "systemd-3-link",
        ] {
            assert_eq!(generation_number("system", link), None, "{link}");
        }
    }

    #[test]
    fn a_shared_closure_reports_its_newest_generation() {
        let closure = PathBuf::from("/nix/store/a-nixos-system");
        let generations = Generations::new(vec![
            Generation {
                number: 3,
                closure: closure.clone(),
            },
            Generation {
                number: 7,
                closure: closure.clone(),
            },
            Generation {
                number: 9,
                closure: PathBuf::from("/nix/store/b-nixos-system"),
            },
        ]);

        assert_eq!(generations.number_of(&closure), Some(7));
        assert_eq!(
            generations.number_of(Path::new("/nix/store/c-nixos-system")),
            None
        );
    }
}
