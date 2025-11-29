{
  description = "Modern reboot detector for NixOS (forked and fixed)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        ###########################
        # Development shell       #
        ###########################
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            rustc
            cargo

            clippy
            rustfmt

            pkg-config
            gdb

            # Formatting for Nix
            nixpkgs-fmt
          ];
        };

        ###########################
        # Build the Rust package  #
        ###########################
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "nixos-needsreboot";
          version = "0.2.0";

          src = ./.;

          # If you vendor dependencies, add:
          # cargoLock = {
          #   lockFile = ./Cargo.lock;
          #   outputHashes = {
          #     "vendor" = "<vendorSha256>";
          #   };
          # };

          cargoLock.lockFile = ./Cargo.lock;

          # Required for most Rust CLI tools
          nativeBuildInputs = [
            pkgs.pkg-config
          ];
        };

        ###########################
        # nix run .               #
        ###########################
        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      }
    );
}
