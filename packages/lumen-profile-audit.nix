{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-profile-audit";
  version = "0.1.0";
  src = ../res/lumen-desktop/profiles;
  cargoLock.lockFile = ../res/lumen-desktop/profiles/Cargo.lock;
}
