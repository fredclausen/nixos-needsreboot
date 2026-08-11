//! Comparing the reboot-worthy modules of two NixOS system closures.

use std::{cmp::Ordering, error, fmt, fs, io, path::Path, path::PathBuf};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// Why two closures could not be compared.
#[derive(Debug)]
pub enum CompareError {
    /// The module symlink inside a closure could not be read.
    UnreadableLink {
        /// The symlink that could not be read.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
    /// The module symlink resolved to a non-UTF-8 path.
    NonUtf8Link {
        /// The symlink whose target is not valid UTF-8.
        path: PathBuf,
    },
    /// A symlink target was too short to name a store package.
    NotAStorePath {
        /// The offending target.
        target: String,
    },
    /// A store path did not carry a recognisable version.
    UnparsableVersion {
        /// The module whose version could not be read.
        module: ModuleType,
        /// The store path that was searched.
        store_path: String,
    },
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnreadableLink { path, source } => {
                write!(f, "Failed to read symlink at {}: {source}", path.display())
            }
            Self::NonUtf8Link { path } => {
                write!(f, "Symlink path contains invalid UTF-8: {}", path.display())
            }
            Self::NotAStorePath { target } => write!(
                f,
                "Cannot determine module directory from '{target}'; \
                 expected '/nix/store/<hash>-<pkg>'"
            ),
            Self::UnparsableVersion { module, store_path } => {
                write!(f, "Failed to get {module} version from {store_path}")
            }
        }
    }
}

impl error::Error for CompareError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::UnreadableLink { source, .. } => Some(source),
            Self::NonUtf8Link { .. }
            | Self::NotAStorePath { .. }
            | Self::UnparsableVersion { .. } => None,
        }
    }
}

/// A component of a system closure whose upgrade requires a reboot.
#[derive(EnumIter, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    /// The Linux kernel.
    LinuxKernel,
    /// systemd, i.e. PID 1.
    Systemd,
}

// for printing messages
impl fmt::Display for ModuleType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::LinuxKernel => write!(f, "Linux Kernel"),
            Self::Systemd => write!(f, "Systemd"),
        }
    }
}

impl ModuleType {
    /// Name of the symlink inside a NixOS system closure that points at this
    /// module.
    #[must_use]
    pub const fn link_name(self) -> &'static str {
        match self {
            Self::LinuxKernel => "kernel",
            Self::Systemd => "systemd",
        }
    }

    /// Substring separating the store hash from the version in this module's
    /// store path name.
    #[must_use]
    pub const fn version_marker(self) -> &'static str {
        match self {
            Self::LinuxKernel => "-linux-",
            Self::Systemd => "-systemd-",
        }
    }

    /// Resolve `<system_path>/<link_name>` down to the `/nix/store/<hash>-<pkg>`
    /// directory that owns it.
    ///
    /// The `kernel` symlink points at a file *inside* the package
    /// (`.../bzImage`), while `systemd` points at the package directory itself,
    /// so the target is always truncated to the store directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the symlink cannot be read, its target is not valid
    /// UTF-8, or the target does not look like a store path.
    pub fn store_path(self, system_path: &Path) -> Result<String, CompareError> {
        debug!("Getting nix store path for module: {self}");

        let link_path = system_path.join(self.link_name());

        debug!("Reading symlink at path: {}", link_path.display());

        let target = fs::read_link(&link_path).map_err(|source| CompareError::UnreadableLink {
            path: link_path.clone(),
            source,
        })?;

        let Ok(target) = target.into_os_string().into_string() else {
            return Err(CompareError::NonUtf8Link { path: link_path });
        };

        let store_path = store_directory(&target)?;

        debug!("Nix store path for module {self}: {store_path}");

        Ok(store_path)
    }

    /// Pull the version out of a `/nix/store/<hash>-<name>-<version>` path.
    #[must_use]
    pub fn extract_version(self, store_path: &str) -> Option<String> {
        debug!("Extracting {self} version from path: {store_path}");

        let file_name = Path::new(store_path).file_name()?.to_str()?;
        let (_, version) = file_name.split_once(self.version_marker())?;

        Some(version.to_string())
    }
}

/// Truncate a path pointing somewhere inside a store package to the package
/// directory itself, i.e. `/nix/store/<hash>-<pkg>`.
///
/// # Errors
///
/// Returns an error if the path has too few components to name a store
/// package.
fn store_directory(path: &str) -> Result<String, CompareError> {
    let parts: Vec<&str> = path.split('/').collect();

    // Expect: [ "", "nix", "store", "<hash>-<pkg>", ... ]
    let Some(slice) = parts.get(1..4) else {
        return Err(CompareError::NotAStorePath {
            target: path.to_string(),
        });
    };

    Ok(format!("/{}", slice.join("/")))
}

/// Split `6.19-rc4` into its base version and a release-candidate rank.
///
/// A final release gets `u64::MAX` so that it outranks every candidate of the
/// same base version: `6.19-rc4 < 6.19`.
fn split_release_candidate(version: &str) -> (&str, u64) {
    match version.split_once("-rc") {
        Some((base, candidate)) => (base, candidate.parse().unwrap_or(0)),
        None => (version, u64::MAX),
    }
}

/// Compare two dot-separated version strings segment by segment.
///
/// The first differing segment decides the result. Segments are compared
/// numerically when both sides parse as integers and lexically otherwise. A
/// missing trailing segment counts as zero, so `6.19` and `6.19.0` are equal
/// and `6.19 -> 6.19.1` is an upgrade.
fn compare_dotted(old: &str, new: &str) -> Ordering {
    let mut old_segments = old.split('.');
    let mut new_segments = new.split('.');

    loop {
        let (old_segment, new_segment) = match (old_segments.next(), new_segments.next()) {
            (None, None) => return Ordering::Equal,
            (old_segment, new_segment) => (old_segment.unwrap_or("0"), new_segment.unwrap_or("0")),
        };

        let ordering = match (old_segment.parse::<u64>(), new_segment.parse::<u64>()) {
            (Ok(old_number), Ok(new_number)) => old_number.cmp(&new_number),
            _ => old_segment.cmp(new_segment),
        };

        if ordering != Ordering::Equal {
            return ordering;
        }
    }
}

/// Order two `major.minor.patch[-rcN]` versions.
#[must_use]
pub fn compare_versions(old: &str, new: &str) -> Ordering {
    let (old_base, old_candidate) = split_release_candidate(old);
    let (new_base, new_candidate) = split_release_candidate(new);

    match compare_dotted(old_base, new_base) {
        Ordering::Equal => old_candidate.cmp(&new_candidate),
        ordering => ordering,
    }
}

/// Compare the kernel and systemd of two NixOS system closures and return one
/// human-readable reason per module that got newer.
///
/// # Errors
///
/// Returns an error if either closure cannot be inspected, or if a module's
/// version cannot be determined. A failure is never reported as "no upgrade":
/// silently concluding that would leave a machine running an old kernel.
pub fn upgrades_available(
    old_system: &Path,
    new_system: &Path,
) -> Result<Vec<String>, CompareError> {
    let mut reason = vec![];

    for module in ModuleType::iter() {
        debug!("Checking module: {module}");

        let old_store_path = module.store_path(old_system)?;
        let new_store_path = module.store_path(new_system)?;

        let old_module_version = module.extract_version(&old_store_path).ok_or_else(|| {
            CompareError::UnparsableVersion {
                module,
                store_path: old_store_path.clone(),
            }
        })?;
        let new_module_version = module.extract_version(&new_store_path).ok_or_else(|| {
            CompareError::UnparsableVersion {
                module,
                store_path: new_store_path.clone(),
            }
        })?;

        debug!("{module}: booted {old_module_version}, activated {new_module_version}");

        if compare_versions(&old_module_version, &new_module_version) == Ordering::Less {
            reason.push(format!(
                "{module} ({old_module_version} -> {new_module_version})\n"
            ));
        }
    }

    Ok(reason)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
// Panicking on a failed setup step is the correct failure mode for a test; the
// crate-level `deny(clippy::unwrap_used)` is aimed at production code.
mod tests {
    use super::{compare_dotted, compare_versions, split_release_candidate, store_directory};
    use super::{CompareError, ModuleType};
    use std::cmp::Ordering;

    #[test]
    fn the_first_differing_segment_decides_the_ordering() {
        assert_eq!(compare_versions("6.18.33", "6.18.41"), Ordering::Less);
        assert_eq!(compare_versions("6.18.41", "6.18.33"), Ordering::Greater);
        assert_eq!(compare_versions("6.18.41", "6.18.41"), Ordering::Equal);

        // A newer patch level must not mask an older minor: this is a
        // downgrade, not an upgrade.
        assert_eq!(compare_versions("6.19.3", "6.18.41"), Ordering::Greater);
        assert_eq!(compare_versions("6.18.41", "6.19.3"), Ordering::Less);
    }

    #[test]
    fn a_missing_trailing_segment_counts_as_zero() {
        // nixpkgs ships a fresh kernel line as "6.19" and its first point
        // release as "6.19.1"; that bump must not be missed.
        assert_eq!(compare_versions("6.19", "6.19.1"), Ordering::Less);
        assert_eq!(compare_versions("6.19.1", "6.19"), Ordering::Greater);
        assert_eq!(compare_versions("6.19", "6.19.0"), Ordering::Equal);
    }

    #[test]
    fn a_release_candidate_ranks_below_its_final_release() {
        assert_eq!(compare_versions("6.19-rc4", "6.19"), Ordering::Less);
        assert_eq!(compare_versions("6.19", "6.19-rc4"), Ordering::Greater);
        assert_eq!(compare_versions("6.19-rc1", "6.19-rc4"), Ordering::Less);
        assert_eq!(compare_versions("6.19-rc4", "6.19-rc4"), Ordering::Equal);
        assert_eq!(compare_versions("6.18.41", "6.19-rc1"), Ordering::Less);
    }

    #[test]
    fn non_numeric_segments_fall_back_to_lexical_ordering() {
        assert_eq!(compare_versions("6.18.a", "6.18.b"), Ordering::Less);
        assert_eq!(compare_versions("6.18.b", "6.18.a"), Ordering::Greater);
    }

    /// An unparsable release-candidate number must not make the candidate
    /// outrank its own final release.
    #[test]
    fn a_malformed_release_candidate_ranks_below_everything() {
        assert_eq!(split_release_candidate("6.19-rcX"), ("6.19", 0));
        assert_eq!(split_release_candidate("6.19-rc"), ("6.19", 0));
        assert_eq!(compare_versions("6.19-rcX", "6.19"), Ordering::Less);
        assert_eq!(compare_versions("6.19-rcX", "6.19-rc1"), Ordering::Less);
    }

    #[test]
    fn a_final_release_outranks_every_candidate_of_the_same_base() {
        assert_eq!(split_release_candidate("6.19"), ("6.19", u64::MAX));
        assert_eq!(split_release_candidate("6.19-rc4"), ("6.19", 4));
    }

    /// Only the first `-rc` splits, so a base version may not silently absorb
    /// a second occurrence.
    #[test]
    fn only_the_first_release_candidate_marker_splits() {
        assert_eq!(split_release_candidate("6.19-rc4-rc5"), ("6.19", 0));
    }

    #[test]
    fn compare_dotted_ignores_release_candidates_entirely() {
        assert_eq!(compare_dotted("6.19", "6.19.0"), Ordering::Equal);
        assert_eq!(compare_dotted("6", "6.0.0.0"), Ordering::Equal);
        assert_eq!(compare_dotted("6.18", "6.19"), Ordering::Less);
    }

    /// An empty segment is not a number, so the comparison falls back to
    /// lexical ordering rather than panicking.
    #[test]
    fn empty_segments_do_not_panic() {
        assert_eq!(compare_dotted("6..1", "6..1"), Ordering::Equal);
        assert_eq!(compare_versions("6..1", "6.1.1"), Ordering::Less);
        assert_eq!(compare_versions("", ""), Ordering::Equal);
    }

    /// A segment too large for `u64` must not panic; it degrades to a lexical
    /// comparison.
    #[test]
    fn a_segment_wider_than_u64_does_not_panic() {
        let huge = "99999999999999999999999999";
        assert_eq!(compare_versions(huge, huge), Ordering::Equal);
        assert_eq!(
            compare_versions("6.1", &format!("6.{huge}")),
            Ordering::Less
        );
    }

    /// Ordering must be antisymmetric, or a downgrade could read as an
    /// upgrade. Every pair is checked in both directions.
    #[test]
    fn the_ordering_is_antisymmetric() {
        let versions = [
            "6.18.33", "6.18.41", "6.19", "6.19.0", "6.19.1", "6.19-rc1", "6.19-rc4", "7.1.7",
            "7.1.8", "260.2", "261.1",
        ];

        for old in versions {
            for new in versions {
                assert_eq!(
                    compare_versions(old, new),
                    compare_versions(new, old).reverse(),
                    "{old} vs {new} is not antisymmetric"
                );
            }
        }
    }

    /// Ordering must be transitive, or the "is this newer" question has no
    /// consistent answer.
    #[test]
    fn the_ordering_is_transitive() {
        let versions = [
            "6.18.33", "6.18.41", "6.19", "6.19.0", "6.19.1", "6.19-rc1", "6.19-rc4", "7.1.7",
            "7.1.8",
        ];

        for a in versions {
            for b in versions {
                for c in versions {
                    if compare_versions(a, b) == Ordering::Less
                        && compare_versions(b, c) == Ordering::Less
                    {
                        assert_eq!(
                            compare_versions(a, c),
                            Ordering::Less,
                            "{a} < {b} < {c} but not {a} < {c}"
                        );
                    }
                }
            }
        }
    }

    /// The real kernel bump this machine was sitting on when these tests were
    /// written.
    #[test]
    fn a_real_nixos_kernel_bump_is_an_upgrade() {
        assert_eq!(compare_versions("7.1.7", "7.1.8"), Ordering::Less);
        assert_eq!(compare_versions("261.1", "261.1"), Ordering::Equal);
    }

    #[test]
    fn store_directory_truncates_to_the_package() {
        assert_eq!(
            store_directory("/nix/store/abc-linux-6.18.41/bzImage").unwrap(),
            "/nix/store/abc-linux-6.18.41"
        );
        assert_eq!(
            store_directory("/nix/store/abc-systemd-260.2").unwrap(),
            "/nix/store/abc-systemd-260.2"
        );
        assert!(matches!(
            store_directory("/nix/store"),
            Err(CompareError::NotAStorePath { .. })
        ));
    }

    /// A relative symlink target has too few components to name a package and
    /// must be rejected rather than silently truncated.
    #[test]
    fn store_directory_rejects_a_target_that_cannot_name_a_package() {
        assert!(matches!(
            store_directory("bzImage"),
            Err(CompareError::NotAStorePath { .. })
        ));
        assert!(matches!(
            store_directory(""),
            Err(CompareError::NotAStorePath { .. })
        ));
    }

    #[test]
    fn extract_version_reads_the_trailing_version() {
        assert_eq!(
            ModuleType::LinuxKernel
                .extract_version("/nix/store/abc-linux-6.18.41")
                .unwrap(),
            "6.18.41"
        );
        assert_eq!(
            ModuleType::Systemd
                .extract_version("/nix/store/abc-systemd-260.2")
                .unwrap(),
            "260.2"
        );
        assert!(ModuleType::LinuxKernel
            .extract_version("/nix/store/abc-systemd-260.2")
            .is_none());
    }

    /// `split_once` takes the *first* marker, so a hardened or otherwise
    /// suffixed kernel yields the whole remainder as its version. Both sides
    /// of a comparison are built the same way, so this still orders correctly
    /// within a variant; the test pins the behaviour so a change is deliberate.
    #[test]
    fn a_variant_kernel_keeps_its_suffix_in_the_version() {
        assert_eq!(
            ModuleType::LinuxKernel
                .extract_version("/nix/store/abc-linux-hardened-6.12.1")
                .unwrap(),
            "hardened-6.12.1"
        );
        assert_eq!(
            compare_versions("hardened-6.12.1", "hardened-6.12.2"),
            Ordering::Less
        );
    }

    #[test]
    fn link_name_and_version_marker_are_stable() {
        assert_eq!(ModuleType::LinuxKernel.link_name(), "kernel");
        assert_eq!(ModuleType::Systemd.link_name(), "systemd");
        assert_eq!(ModuleType::LinuxKernel.version_marker(), "-linux-");
        assert_eq!(ModuleType::Systemd.version_marker(), "-systemd-");
    }

    /// The reason strings are user-visible and land in the flag file.
    #[test]
    fn module_names_render_for_humans() {
        assert_eq!(ModuleType::LinuxKernel.to_string(), "Linux Kernel");
        assert_eq!(ModuleType::Systemd.to_string(), "Systemd");
    }
}
