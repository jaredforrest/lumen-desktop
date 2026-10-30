{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-spaces";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource { root = ../res/lumen-desktop; fileset = pkgs.lib.fileset.unions [ ../res/lumen-desktop/spaces ../res/lumen-desktop/window-policy ]; };
  sourceRoot = "source/spaces";
  cargoLock.lockFile = ../res/lumen-desktop/spaces/Cargo.lock;
}
