{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-lifecycle-native-fixture";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource { root = ../../..; fileset = pkgs.lib.fileset.unions [ ./. ../../../res/lumen-desktop/window-policy ]; };
  sourceRoot = "source/tests/desktop/lifecycle-native-rust";
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
}
