{
  description = "Modern reboot detector for NixOS (forked and fixed)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks.url = "github:cachix/git-hooks.nix";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      git-hooks,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        #################################
        ## Package definition
        #################################
        packages = rec {
          # Main package
          nixos-needsreboot = pkgs.rustPlatform.buildRustPackage {
            pname = "nixos-needsreboot";
            version = "0.2.2";

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
          program = "${self.packages.${system}.default}/bin/nixos-needsreboot";
        };

        # Also expose top-level defaults (nice for users)
        defaultPackage = self.packages.${system}.default;
        defaultApp = self.apps.${system}.default;

        checks.pre-commit-check = git-hooks.lib.${system}.run {
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              # keep all files, including dotfiles
              true;
          };

          excludes = [
            "^res/"
            "^./res/"
            "^typos\\.toml$"
            "^speed_tests/.*\\.txt$"
            "^Documents/.*"
          ];

          hooks = {
            # Built-in git-hooks.nix hooks
            check-yaml.enable = true;
            end-of-file-fixer.enable = true;
            trailing-whitespace = {
              enable = true;
              entry = "${pkgs.python3Packages.pre-commit-hooks}/bin/trailing-whitespace-fixer";
            };

            mixed-line-ending = {
              enable = true;
              entry = "${pkgs.python3Packages.pre-commit-hooks}/bin/mixed-line-ending";
              args = [ "--fix=auto" ];
            };

            check-executables-have-shebangs.enable = true;
            check-shebang-scripts-are-executable.enable = true;
            black.enable = true;
            flake8.enable = true;
            nixfmt.enable = true;
            hadolint.enable = true;
            shellcheck.enable = true;
            prettier.enable = true;

            # Hooks that need system packages
            codespell = {
              enable = true;
              entry = "${pkgs.codespell}/bin/codespell";
              args = [ "--ignore-words=.dictionary.txt" ];
              files = "\\.([ch]|cpp|rs|py|sh|txt|md|toml|yaml|yml)$";
            };

            check-github-actions = {
              enable = true;
              entry = "${pkgs.check-jsonschema}/bin/check-jsonschema";
              args = [
                "--builtin-schema"
                "github-actions"
              ];
              files = "^\\.github/actions/.*\\.ya?ml$";
              pass_filenames = true;
            };

            check-github-workflows = {
              enable = true;
              entry = "${pkgs.check-jsonschema}/bin/check-jsonschema";
              args = [
                "--builtin-schema"
                "github-workflows"
              ];
              files = "^\\.github/workflows/.*\\.ya?ml$";
              pass_filenames = true;
            };

            # Rust hooks
            rustfmt = {
              enable = true;
              entry = "${pkgs.cargo}/bin/cargo";
              args = [
                "fmt"
                "--all"
              ];
            };
            clippy = {
              enable = true;
              entry = "${pkgs.cargo}/bin/cargo";
              args = [
                "clippy"
                "--workspace"
                "--all-targets"
              ];
            };
          };
        };

        #################################
        ## Dev Shell
        #################################
        devShells.default =
          let
            inherit (self.checks.${system}.pre-commit-check) shellHook enabledPackages;

          in
          pkgs.mkShell {
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

            shellHook = ''
              # Run git-hooks.nix setup (creates .pre-commit-config.yaml)
              ${shellHook}

              # Your own extras
              alias pre-commit="pre-commit run --all-files"
              alias xtask="cargo run -p xtask --"
            '';

          };
      }
    );
}
