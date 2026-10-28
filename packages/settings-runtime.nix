{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-nix-settings-runtime";
  version = "0.1.0";
  src = ../res/lumen-desktop/nix-settings;
  cargoLock.lockFile = ../res/lumen-desktop/nix-settings/Cargo.lock;
}
