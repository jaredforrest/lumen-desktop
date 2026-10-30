{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-open-folder";
  version = "0.1.0";
  src = ../res/lumen-desktop/open-folder;
  cargoLock.lockFile = ../res/lumen-desktop/open-folder/Cargo.lock;
  nativeCheckInputs = [ pkgs.glib ];
}
