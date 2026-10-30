{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-dock-drop";
  version = "0.1.0";
  src = ../res/lumen-desktop/dock-drop;
  cargoLock.lockFile = ../res/lumen-desktop/dock-drop/Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.glib ];
}
