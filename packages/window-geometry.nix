{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-window-geometry";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource { root = ../res/lumen-desktop; fileset = pkgs.lib.fileset.unions [ ../res/lumen-desktop/window-geometry ../res/lumen-desktop/window-policy ]; };
  sourceRoot = "source/window-geometry";
  cargoLock.lockFile = ../res/lumen-desktop/window-geometry/Cargo.lock;
}
