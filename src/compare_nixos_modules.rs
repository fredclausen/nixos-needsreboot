use std::{cmp::Ordering, error::Error, fmt, fs, path::Path};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(EnumIter, Clone, Copy)]
enum ModuleType {
    LinuxKernel,
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
    const fn link_name(self) -> &'static str {
        match self {
            Self::LinuxKernel => "kernel",
            Self::Systemd => "systemd",
        }
    }

    /// Substring separating the store hash from the version in this module's
    /// store path name.
    const fn version_marker(self) -> &'static str {
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
    fn store_path(self, system_path: &Path) -> Result<String, Box<dyn Error>> {
        debug!("Getting nix store path for module: {self}");

        let link_path = system_path.join(self.link_name());

        debug!("Reading symlink at path: {}", link_path.display());

        let target = match fs::read_link(&link_path) {
            Ok(p) => p,
            Err(e) => {
                return Err(
                    format!("Failed to read symlink at {}: {}", link_path.display(), e).into(),
                );
            }
        };

        let Ok(target) = target.into_os_string().into_string() else {
            return Err(format!(
                "Symlink path contains invalid UTF-8: {}",
                link_path.display()
            )
            .into());
        };

        let store_path = store_directory(&target)?;

        debug!("Nix store path for module {self}: {store_path}");

        Ok(store_path)
    }

    /// Pull the version out of a `/nix/store/<hash>-<name>-<version>` path.
    fn extract_version(self, store_path: &str) -> Option<String> {
        debug!("Extracting {self} version from path: {store_path}");

        let file_name = Path::new(store_path).file_name()?.to_str()?;
        let (_, version) = file_name.split_once(self.version_marker())?;

        Some(version.to_string())
    }
}

/// Truncate a path pointing somewhere inside a store package to the package
/// directory itself, i.e. `/nix/store/<hash>-<pkg>`.
fn store_directory(path: &str) -> Result<String, Box<dyn Error>> {
    let parts: Vec<&str> = path.split('/').collect();

    // Expect: [ "", "nix", "store", "<hash>-<pkg>", ... ]
    let Some(slice) = parts.get(1..4) else {
        return Err(format!(
            "Cannot determine module directory from '{path}'; \
             expected '/nix/store/<hash>-<pkg>'"
        )
        .into());
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
fn compare_versions(old: &str, new: &str) -> Ordering {
    let (old_base, old_candidate) = split_release_candidate(old);
    let (new_base, new_candidate) = split_release_candidate(new);

    match compare_dotted(old_base, new_base) {
        Ordering::Equal => old_candidate.cmp(&new_candidate),
        ordering => ordering,
    }
}

/// Compare the kernel and systemd of two NixOS system closures and return one
/// human-readable reason per module that got newer.
pub fn upgrades_available(
    old_system: &Path,
    new_system: &Path,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut reason = vec![];

    for module in ModuleType::iter() {
        debug!("Checking module: {module}");

        let old_store_path = module
            .store_path(old_system)
            .map_err(|e| format!("Failed to get old nix store path for {module}: {e}"))?;
        let new_store_path = module
            .store_path(new_system)
            .map_err(|e| format!("Failed to get new nix store path for {module}: {e}"))?;

        let old_module_version = module
            .extract_version(&old_store_path)
            .ok_or_else(|| format!("Failed to get old {module} version from {old_store_path}"))?;
        let new_module_version = module
            .extract_version(&new_store_path)
            .ok_or_else(|| format!("Failed to get new {module} version from {new_store_path}"))?;

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
    use super::{compare_versions, store_directory, upgrades_available, ModuleType};
    use crate::test_support::fake_system;
    use std::cmp::Ordering;
    use tempfile::TempDir;

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

    #[test]
    fn a_module_is_reported_at_most_once() {
        let tmp = TempDir::new().unwrap();

        // Two segments move at the same time; that is still one upgrade.
        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.5", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.19.41", "260.2");

        let reasons = upgrades_available(&old, &new).unwrap();
        assert_eq!(reasons, vec!["Linux Kernel (6.18.5 -> 6.19.41)\n"]);
    }

    #[test]
    fn a_kernel_downgrade_is_not_reported_as_an_upgrade() {
        let tmp = TempDir::new().unwrap();

        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.19.3", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.18.41", "260.2");

        assert!(upgrades_available(&old, &new).unwrap().is_empty());
    }

    #[test]
    fn kernel_upgrade_is_detected() {
        let tmp = TempDir::new().unwrap();

        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.33", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.18.41", "260.2");

        let reasons = upgrades_available(&old, &new).unwrap();
        assert_eq!(reasons, vec!["Linux Kernel (6.18.33 -> 6.18.41)\n"]);
    }

    #[test]
    fn identical_kernel_and_systemd_report_nothing() {
        let tmp = TempDir::new().unwrap();

        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.41", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.18.41", "260.2");

        assert_ne!(old, new);
        assert!(upgrades_available(&old, &new).unwrap().is_empty());
    }

    #[test]
    fn systemd_upgrade_is_detected() {
        let tmp = TempDir::new().unwrap();

        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.41", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.18.41", "261.1");

        let reasons = upgrades_available(&old, &new).unwrap();
        assert_eq!(reasons, vec!["Systemd (260.2 -> 261.1)\n"]);
    }

    #[test]
    fn missing_symlink_is_an_error_not_a_silent_pass() {
        let tmp = TempDir::new().unwrap();
        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.41", "260.2");
        let missing = tmp.path().join("nope");

        assert!(upgrades_available(&old, &missing).is_err());
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
        assert!(store_directory("/nix/store").is_err());
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
}
