# nixos-needsreboot

`nixos-needsreboot` determines whether a NixOS system requires a reboot
after an upgrade. It checks kernel, systemd, and service-level changes
and optionally writes a reboot-required flag file.

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
```

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
if nixos-needsreboot --dry-run; then
    echo "A reboot is required."
else
    echo "No reboot is needed."
fi
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

```ini
[Unit]
Description=Check if system reboot is needed

[Service]
Type=oneshot
ExecStart=/run/current-system/sw/bin/nixos-needsreboot

[Install]
WantedBy=multi-user.target
```

## ❤️ Attribution

This project is based on the excellent original work at:
➡️ **[The FOSS Guy](https://github.com/thefossguy/nixos-needsreboot)**
