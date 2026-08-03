use std::{error::Error, fmt, fs, path::Path};
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

        let (mut old_module_version, mut new_module_version) =
            (old_module_version, new_module_version);

        if old_module_version != new_module_version {
            if old_module_version.len() != new_module_version.len() {
                let old_has_rc = old_module_version.contains("-rc");
                let new_has_rc = new_module_version.contains("-rc");

                match (old_has_rc, new_has_rc) {
                    (true, false) => {
                        old_module_version = old_module_version.replace("-rc", ".");
                        new_module_version.push_str(".0");
                    }
                    (false, true) => {
                        new_module_version = new_module_version.replace("-rc", ".");
                        old_module_version.push_str(".0");
                    }
                    (true, true) => {
                        old_module_version = old_module_version.replace("-rc", ".");
                        new_module_version = new_module_version.replace("-rc", ".");
                    }
                    (false, false) => {}
                }
            }

            let old_parts: Vec<&str> = old_module_version.split('.').collect();
            let new_parts: Vec<&str> = new_module_version.split('.').collect();

            for (old, new) in old_parts.iter().zip(new_parts.iter()) {
                // Try numeric comparison first
                let old_num = old.parse::<u64>().ok();
                let new_num = new.parse::<u64>().ok();

                match (old_num, new_num) {
                    // Both parts are numeric: compare numerically
                    (Some(o), Some(n)) => {
                        if n > o {
                            reason.push(format!(
                                "{module} ({old_module_version} -> {new_module_version})\n"
                            ));
                        }
                    }

                    // Non-numeric segments: fallback to string comparison
                    _ => {
                        if new > old {
                            reason.push(format!(
                                "{module} ({old_module_version} -> {new_module_version})\n"
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(reason)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
// Panicking on a failed setup step is the correct failure mode for a test; the
// crate-level `deny(clippy::unwrap_used)` is aimed at production code.
mod tests {
    use super::{store_directory, upgrades_available, ModuleType};
    use crate::test_support::fake_system;
    use tempfile::TempDir;

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
