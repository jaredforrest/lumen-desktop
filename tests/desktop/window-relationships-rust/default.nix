{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-window-relationships-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
