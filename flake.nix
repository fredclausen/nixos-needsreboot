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
        #################################
        ## Dev Shell
        #################################
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustup
            cargo
            rustc
            clippy
            rustfmt

            gdb
            pkg-config

            nixpkgs-fmt
          ];
        };

        #################################
        ## Package definition
        #################################
        packages = rec {
          # Main package
          nixos-needsreboot = pkgs.rustPlatform.buildRustPackage {
            pname = "nixos-needsreboot";
            version = "0.2.0";

            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              # Usually not needed but harmless
              pkgs.pkg-config
            ];

            meta = with pkgs.lib; {
              description = "Modern reboot detector for NixOS (forked and fixed)";
              homepage = "https://github.com/fredclausen/nixos-needsreboot";
              license = licenses.mit;
              platforms = platforms.linux;
              maintainers = [ maintainers.fredclausen ];
            };
          };

          # Alias `default` → main package
          default = nixos-needsreboot;
        };

        #################################
        ## nix run .
        #################################
        apps.default = {
          type = "app";
          program =
            "${self.packages.${system}.default}/bin/nixos-needsreboot";
        };

        # Also expose top-level defaults (nice for users)
        defaultPackage = self.packages.${system}.default;
        defaultApp     = self.apps.${system}.default;
      }
    );
}
