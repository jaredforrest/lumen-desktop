{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-nix-settings-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
