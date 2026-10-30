{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-capture-native-fixture";
  version = "0.1.0";
  src = ../.;
  cargoRoot = "capture-native-rust";
  buildAndTestSubdir = "capture-native-rust";
  cargoLock.lockFile = ./Cargo.lock;
}
