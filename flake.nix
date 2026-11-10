{
  description = "Modern reboot detector for NixOS (forked and fixed)";

  inputs = {
    # renovate: datasource=git-refs depName=nixpkgs packageName=https://github.com/NixOS/nixpkgs versioning=regex:^nixos-(?<major>\d+)\.(?<minor>\d+)$
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
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
    {
      #################################
      ## Overlay
      #################################
      # Builds against the consumer's own nixpkgs, so a NixOS config can
      # install this without pulling this flake's pinned nixpkgs into its
      # closure (and can set `inputs.nixpkgs.follows` to drop it from its
      # lock entirely).
      overlays.default = final: _prev: {
        nixos-needsreboot = final.callPackage ./package.nix { };
      };
    }
    // flake-utils.lib.eachDefaultSystem (
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
          nixos-needsreboot = pkgs.callPackage ./package.nix { };

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
              # NOTE: deliberately no `rustup`.  Its shims take PATH priority
              # over the nixpkgs toolchain below, and the toolchains rustup
              # downloads are non-FHS binaries that cannot exec on NixOS
              # (missing /lib64/ld-linux-x86-64.so.2), so `cargo` fails with
              # "No such file or directory".  Use the pinned nixpkgs toolchain.
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
