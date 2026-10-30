{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-clipboard-history";
  version = "0.1.0";
  src = ../res/lumen-desktop/clipboard-history;
  cargoLock.lockFile = ../res/lumen-desktop/clipboard-history/Cargo.lock;
}
