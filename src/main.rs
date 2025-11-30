// Original code from https://github.com/thefossguy/nixos-needsreboot
// Forked and updated by Fred Clausen https://github.com/fredclausen/nixos-needsreboot/

#![deny(
    clippy::pedantic,
    //clippy::cargo,
    clippy::nursery,
    clippy::style,
    clippy::correctness,
    clippy::all,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[macro_use]
extern crate log;

use env_logger::Builder;
use log::LevelFilter;
use sdre_rust_logging::SetupLogging;
use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

mod compare_nixos_modules;

pub static OLD_SYSTEM_PATH: &str = "/run/booted-system";
pub static NEW_SYSTEM_PATH: &str = "/nix/var/nix/profiles/system";
pub static NIXOS_NEEDS_REBOOT: &str = "/run/reboot-required";

fn enable_logging(env_args: &[String]) {
    if env_args.contains(&"--help".to_string()) {
        print_help();
        std::process::exit(0);
    }

    if env_args.contains(&"--logging-test".to_string()) || env_args.contains(&"--debug".to_string())
    {
        if env_args.contains(&"--debug".to_string()) {
            "DEBUG".enable_logging();
        } else {
            "INFO".enable_logging();
        }
    } else {
        let _ = Builder::new()
            .format(|buf, record| writeln!(buf, "{}", record.args()))
            .filter(None, LevelFilter::Info)
            .try_init();
    }
}

fn print_help() {
    println!("nixos-needsreboot - Determine if a NixOS system reboot is required");
    println!();
    println!("USAGE:");
    println!("  nixos-needsreboot [--dry-run] [--no-force-recompute] [--help] [--version] [--logging-test] [--debug]");
    println!();
    println!("OPTIONS:");
    println!("  --dry-run               Print the reasons for needing a reboot without creating the reboot file");
    println!("  --no-force-recompute    Do not recompute the reboot requirement if the reboot file already exists");
    println!("  --help                  Print this help message");
    println!("  --version               Print version information");
    println!("  --logging-test          Enable logging for testing purposes");
    println!("  --debug                 Enable debug logging");
}

fn version(env_args: &[String]) {
    if env_args.contains(&"--version".to_string()) {
        info!("{}: v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
}

fn get_user(dry_run: bool) {
    let user_result = env::var_os("USER");

    let user = user_result.map_or_else(
        || {
            error!("Cannot determine current user");
            std::process::exit(1);
        },
        |val| {
            val.into_string().unwrap_or_else(|_| {
                error!("Cannot convert USER to String");
                std::process::exit(1);
            })
        },
    );

    if user != "root" && !dry_run {
        error!("Please run this as root");
        error!("HINT: use the '--dry-run' option");
        std::process::exit(1);
    }
}

fn verify_nixos_and_paths() -> (String, String) {
    if !Path::new(NEW_SYSTEM_PATH).exists() {
        error!("This binary is intended to run only on NixOS.");
        std::process::exit(1);
    }

    let old_path = format!("{OLD_SYSTEM_PATH}/nixos-version");
    let old_system_id = match fs::read_to_string(&old_path) {
        Ok(id) => id,
        Err(e) => {
            error!("Cannot read old system nixos-version ({old_path}): {e}");
            std::process::exit(1);
        }
    };

    let new_path = format!("{NEW_SYSTEM_PATH}/nixos-version");
    let new_system_id = match fs::read_to_string(&new_path) {
        Ok(id) => id,
        Err(e) => {
            error!("Cannot read new system nixos-version ({new_path}): {e}");
            std::process::exit(1);
        }
    };

    (old_system_id, new_system_id)
}

fn maybe_skip_checks(no_force_recompute: bool) {
    if Path::new(NIXOS_NEEDS_REBOOT).exists() && no_force_recompute {
        let contents = match fs::read_to_string(NIXOS_NEEDS_REBOOT) {
            Ok(c) => c,
            Err(e) => {
                error!("Could not read existing {NIXOS_NEEDS_REBOOT}: {e}");
                std::process::exit(1);
            }
        };

        info!("Reboot needed: {}", contents.trim());

        std::process::exit(0);
    }
}

fn maybe_delete_old_reboot_file(dry_run: bool) {
    if Path::new(NIXOS_NEEDS_REBOOT).exists() && !dry_run {
        match fs::remove_file(NIXOS_NEEDS_REBOOT) {
            Ok(()) => {}
            Err(e) => {
                error!("Could not remove existing {NIXOS_NEEDS_REBOOT}: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn are_we_latest_nixos_generation(old_system_id: &str, new_system_id: &str) {
    if old_system_id == new_system_id {
        info!("You are using the latest NixOS generation, no need to reboot");
        std::process::exit(0);
    }
}

fn generate_reason_for_reboot() -> Vec<String> {
    let reason = match compare_nixos_modules::upgrades_available() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to compute upgrades:\n{e}");
            std::process::exit(1);
        }
    };

    if reason.is_empty() {
        info!("No updates available, moar uptime!!!");
        std::process::exit(0);
    }

    reason
}

fn print_reasons(reason: &[String]) {
    for r in reason {
        info!("Upgrade available: {}", r.trim());
    }
}

fn write_reason_file(reason: &[String]) {
    let mut reason_out = String::new();

    for r in reason {
        reason_out.push_str(r);
    }

    match fs::write(NIXOS_NEEDS_REBOOT, &reason_out) {
        Ok(()) => {}
        Err(e) => {
            error!("Could not write {NIXOS_NEEDS_REBOOT}: {e}");
            std::process::exit(1);
        }
    }
    std::process::exit(2);
}

fn main() {
    let env_args: Vec<String> = env::args().collect();

    enable_logging(&env_args);
    version(&env_args);

    let dry_run = env_args.contains(&"--dry-run".to_string());
    let no_force_recompute = env_args.contains(&"--no-force-recompute".to_string());

    get_user(dry_run);

    let (old_system_id, new_system_id) = verify_nixos_and_paths();

    maybe_skip_checks(no_force_recompute);

    maybe_delete_old_reboot_file(dry_run);

    are_we_latest_nixos_generation(&old_system_id, &new_system_id);

    let reason = generate_reason_for_reboot();

    if dry_run {
        print_reasons(&reason);
    } else {
        write_reason_file(&reason);
    }
}
