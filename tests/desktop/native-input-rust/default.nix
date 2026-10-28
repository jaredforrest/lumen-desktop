{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-native-input-fixtures";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
