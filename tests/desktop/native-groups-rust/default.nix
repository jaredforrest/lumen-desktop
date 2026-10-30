{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-native-groups-fixture";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource { root = ../../..; fileset = pkgs.lib.fileset.unions [ ./. ../../../res/lumen-desktop/spaces ../../../res/lumen-desktop/window-policy ]; };
  sourceRoot = "source/tests/desktop/native-groups-rust";
  cargoLock.lockFile = ./Cargo.lock;
}
