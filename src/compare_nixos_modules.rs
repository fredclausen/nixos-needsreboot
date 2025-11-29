use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::{NEW_SYSTEM_PATH, OLD_SYSTEM_PATH};

#[derive(EnumIter)]
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
    fn get_nix_store_path(&self, use_old_path: bool) -> Result<String, Box<dyn Error>> {
        debug!("Getting nix store path for module: {self}");

        let suffix = match self {
            Self::LinuxKernel => "kernel",
            Self::Systemd => "systemd",
        };

        let strip_suffix = matches!(self, Self::LinuxKernel);

        let system_path = if use_old_path {
            OLD_SYSTEM_PATH
        } else {
            NEW_SYSTEM_PATH
        };

        debug!("System path for module {self}: {system_path} and suffix: {suffix}");

        let link_path: PathBuf = Path::new(&system_path).join(suffix);

        debug!("Reading symlink at path: {}", link_path.display());

        let tmp_module_path_os = match fs::read_link(&link_path) {
            Ok(p) => p,
            Err(e) => {
                return Err(
                    format!("Failed to read symlink at {}: {}", link_path.display(), e).into(),
                );
            }
        };

        let os_string = tmp_module_path_os.into_os_string();

        let Ok(tmp_module_path) = os_string.into_string() else {
            return Err(format!(
                "Symlink path contains invalid UTF-8: {}",
                link_path.display()
            )
            .into());
        };

        let nix_module_path = if strip_suffix {
            let parts: Vec<&str> = tmp_module_path.split('/').collect();

            // Expect: [ "", "nix", "store", "<hash>-<pkg>", ... ]
            let slice = parts.get(1..4);
            let joined = match slice {
                Some(v) => v.join("/"),
                None => {
                    return Err(format!(
                        "Cannot determine module directory from '{tmp_module_path}'; \
                     expected '/nix/store/<hash>-<pkg>'"
                    )
                    .into());
                }
            };

            format!("/{joined}")
        } else {
            tmp_module_path
        };

        debug!("Nix store path for module {self}: {nix_module_path}");

        Ok(nix_module_path)
    }

    fn extract_systemd_version(path: &str) -> Option<String> {
        debug!("Extracting systemd version from path: {path}");
        let file_name = Path::new(path).file_name()?.to_str()?;
        let parts: Vec<&str> = file_name.splitn(2, "-systemd-").collect();

        if parts.len() == 2 {
            Some(parts[1].to_string())
        } else {
            None
        }
    }

    fn extract_kernel_version(path: &str) -> Option<String> {
        debug!("Extracting kernel version from path: {path}");
        let file_name = Path::new(path).file_name()?.to_str()?;

        let parts: Vec<&str> = file_name.split("-linux-").collect();
        if parts.len() == 2 {
            return Some(parts[1].to_string());
        }
        None
    }

    fn get_version(&self) -> Result<(String, String), Box<dyn Error>> {
        debug!("Getting version for module: {self}");

        let old_module_root_path = match self.get_nix_store_path(true) {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Failed to get old nix store path for {self}: {e}").into());
            }
        };

        let new_module_root_path = match self.get_nix_store_path(false) {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Failed to get new nix store path for {self}: {e}").into());
            }
        };

        let old_module_version: String;
        let new_module_version: String;

        match self {
            Self::LinuxKernel => {
                // Build full paths
                old_module_version = Self::extract_kernel_version(&old_module_root_path)
                    .ok_or("Could not extract kernel version")?;

                new_module_version = Self::extract_kernel_version(&new_module_root_path)
                    .ok_or("Could not extract kernel version")?;
            }

            Self::Systemd => {
                // old systemd version
                old_module_version = match Self::extract_systemd_version(&old_module_root_path) {
                    Some(v) => v,
                    None => {
                        return Err(format!(
                            "Failed to get old systemd version from {old_module_root_path}"
                        )
                        .into());
                    }
                };

                // new systemd version
                new_module_version = match Self::extract_systemd_version(&new_module_root_path) {
                    Some(v) => v,
                    None => {
                        return Err(format!(
                            "Failed to get new systemd version from {new_module_root_path}"
                        )
                        .into());
                    }
                };
            }
        }

        Ok((old_module_version, new_module_version))
    }
}

pub fn upgrades_available() -> Result<Vec<String>, Box<dyn Error>> {
    let mut reason = vec![];

    for module in ModuleType::iter() {
        debug!("Checking module: {module}");
        let (mut old_module_version, mut new_module_version) = match module.get_version() {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Failed to get version for module {module}:\n{e}").into());
            }
        };

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
