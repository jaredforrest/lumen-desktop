{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-window-frames-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
