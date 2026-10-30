{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-shortcut-settings";
  version = "0.1.0";
  src = ../res/lumen-desktop/shortcut-settings;
  cargoLock.lockFile = ../res/lumen-desktop/shortcut-settings/Cargo.lock;
  env.RUSTFLAGS = "-D warnings";
}
