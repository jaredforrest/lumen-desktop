{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-search-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
