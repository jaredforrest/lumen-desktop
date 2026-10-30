{ pkgs }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-focus-window";
    version = "0.1.0";
    env.RUSTFLAGS = "-D warnings";
    src = pkgs.lib.fileset.toSource { root = ../res/lumen-desktop; fileset = pkgs.lib.fileset.unions [ ../res/lumen-desktop/focus-window ../res/lumen-desktop/window-policy ]; };
    sourceRoot = "source/focus-window";
    cargoLock.lockFile = ../res/lumen-desktop/focus-window/Cargo.lock;
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-focus-window";
  inherit binary;
  runtimeInputs = [ pkgs.sway ];
}
