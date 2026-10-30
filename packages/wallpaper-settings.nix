{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-wallpaper-settings";
  version = "0.1.0";
  src = ../res/lumen-desktop/wallpaper-settings;
  cargoLock.lockFile = ../res/lumen-desktop/wallpaper-settings/Cargo.lock;
}
