{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-display-visual-fixtures";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
