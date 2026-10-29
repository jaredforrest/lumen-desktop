{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-native-groups-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
}
