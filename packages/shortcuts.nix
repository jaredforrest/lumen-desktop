{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-shortcuts";
  version = "0.1.0";
  src = ../res/lumen-desktop/shortcuts;
  cargoLock.lockFile = ../res/lumen-desktop/shortcuts/Cargo.lock;
  LUMEN_SHORTCUT_DEFAULTS = ../res/lumen-desktop/runtime/shortcut-defaults.json;
}
