{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-shortcut-hints-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
