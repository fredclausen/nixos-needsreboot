# nixos-needsreboot

`nixos-needsreboot` determines whether a NixOS system requires a reboot
after an upgrade. It compares the booted system closure against the
activated one, and optionally writes a reboot-required flag file.

This project is a **maintained fork** of the original work by
[@thefossguy](https://github.com/thefossguy/nixos-needsreboot)

## 📦 Features

- Detects whether a reboot is required based on:
  - Kernel updates
  - Systemd updates
- Can print the reasons without touching the system (`--dry-run`)
- Can avoid recomputing if a reboot flag exists (`--no-force-recompute`)
- Optional debug logging (`--debug`)
- Optional logging mode for CI or testing (`--logging-test`)
- Suitable for systemd units, CI pipelines, and automatic upgrade
  scripts

## 🖥️ CLI Usage

```bash
nixos-needsreboot - Determine if a NixOS system reboot is required

USAGE:
  nixos-needsreboot [--dry-run] [--no-force-recompute] [--help] [--version] [--logging-test] [--debug]

OPTIONS:
  --dry-run               Print the reasons for needing a reboot without creating the reboot file
  --no-force-recompute    Do not recompute the reboot requirement if the reboot file already exists
  --help                  Print this help message
  --version               Print version information
  --logging-test          Enable logging for testing purposes
  --debug                 Enable debug logging

EXIT STATUS:
  0    No reboot is required
  1    The check could not be completed
  2    A reboot is required
```

## 🚦 Exit status

The exit status is the tool's contract; branch on it rather than on the
printed text.

| Status | Meaning                          |
| ------ | -------------------------------- |
| `0`    | No reboot is required            |
| `1`    | The check could not be completed |
| `2`    | A reboot is required             |

`--dry-run` reports the same status as a real run; the only difference is
that it never creates or removes the flag file. Because a required reboot
is a _non-zero_ status, `if nixos-needsreboot; then ...` does **not** mean
"a reboot is required" — test the status explicitly, as below.

Writing the flag file requires root. `--dry-run`, `--help` and `--version`
do not.

## 🔧 Example: Check for updates + show what will change + check reboot flag

```bash
#!/usr/bin/env bash

set -euo pipefail

echo "=== Checking for system updates ==="
sudo nixos-rebuild dry-activate 2>&1 | tee /tmp/nixos-update-preview.log

echo
echo "=== Packages that would change ==="
grep '^activating' -n /tmp/nixos-update-preview.log || echo "No activation changes found."

echo
echo "=== Determining if a reboot is required ==="
status=0
nixos-needsreboot --dry-run || status=$?

case "$status" in
    0) echo "No reboot is needed." ;;
    2) echo "A reboot is required." ;;
    *) echo "Reboot check failed." >&2; exit 1 ;;
esac
```

## ❄️ Using `nixos-needsreboot` as a Flake Input

```nix
{
  inputs.nixos-needsreboot.url = "github:fredclausen/nixos-needsreboot";

  outputs = { self, nixos-needsreboot, ... }:
    let
      pkgs = import <nixpkgs> { system = "x86_64-linux"; };
    in {
      nixosConfigurations.your-hostname = pkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ({ pkgs, ... }: {
            environment.systemPackages = [
              nixos-needsreboot.packages.${pkgs.system}.default
            ];
          })
        ];
      };
    };
}
```

## 📁 Using Inside a Traditional NixOS Configuration

```nix
{
  environment.systemPackages = with pkgs; [
    (import (fetchTarball "https://github.com/fredclausen/nixos-needsreboot/archive/master.tar.gz") { }).defaultPackage.${pkgs.system}
  ];
}
```

## ⚙️ Example Systemd Service

A required reboot exits with status `2`, which systemd would otherwise
record as a failure, so the unit must declare it a success.

```ini
[Unit]
Description=Check if system reboot is needed

[Service]
Type=oneshot
ExecStart=/run/current-system/sw/bin/nixos-needsreboot
# 2 means "a reboot is required", which is a successful check.
SuccessExitStatus=2

[Install]
WantedBy=multi-user.target
```

## 🧪 Development

The dev shell provides the pinned Rust toolchain and the lint hooks:

```bash
nix develop          # or `direnv allow`
cargo test           # unit + integration tests
pre-commit           # aliased to `pre-commit run --all-files`
nix build            # builds the package and reruns the tests in the sandbox
```

CI runs all of the above on every pull request, so a change that breaks
the tests cannot be merged.

Tests live in two places: `src/*.rs` unit tests cover the pure logic
(version ordering, store-path parsing, argument parsing), while `tests/`
covers behaviour through the public API — the reboot decision, the flag
file lifecycle, and the exit-status contract. `tests/common/` builds a
throwaway NixOS-shaped directory tree, so no test needs root or a real
`/nix/store`.

## ❤️ Attribution

This project is based on the excellent original work at:
➡️ **[The FOSS Guy](https://github.com/thefossguy/nixos-needsreboot)**
