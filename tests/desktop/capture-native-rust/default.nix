{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-capture-native-fixture";
  version = "0.1.0";
  src = ../.;
  cargoRoot = "capture-native-rust";
  buildAndTestSubdir = "capture-native-rust";
  cargoLock.lockFile = ./Cargo.lock;
}
