{
  lib,
  rustPlatform,
  pkg-config,
}:
rustPlatform.buildRustPackage {
  pname = "nixos-needsreboot";
  version = "0.3.0";

  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [
    # Usually not needed but harmless
    pkg-config
  ];

  meta = {
    description = "Modern reboot detector for NixOS (forked and fixed)";
    homepage = "https://github.com/fredclausen/nixos-needsreboot";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
    # Inline rather than `lib.maintainers.fredclausen`: there is no such
    # entry in nixpkgs, and the missing attribute is a hard eval error for
    # anything that reads meta (e.g. sbomnix), which tryEval cannot catch.
    maintainers = [
      {
        name = "Fred Clausen";
        github = "fredclausen";
        githubId = 43556888;
      }
    ];
  };
}
