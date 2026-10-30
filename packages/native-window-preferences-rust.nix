{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-native-window-preferences";
  version = "0.1.0";
  src = ../res/lumen-desktop/native-window-preferences;
  cargoLock.lockFile = ../res/lumen-desktop/native-window-preferences/Cargo.lock;
}
