{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-portal-pane-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
