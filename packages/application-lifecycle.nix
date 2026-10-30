{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-application-lifecycle";
  version = "0.1.0";
  env.RUSTFLAGS = "-D warnings";
  src = pkgs.lib.fileset.toSource { root = ../res/lumen-desktop; fileset = pkgs.lib.fileset.unions [ ../res/lumen-desktop/application-lifecycle ../res/lumen-desktop/window-policy ]; };
  sourceRoot = "source/application-lifecycle";
  cargoLock.lockFile = ../res/lumen-desktop/application-lifecycle/Cargo.lock;
}
