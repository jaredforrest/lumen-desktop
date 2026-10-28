{ pkgs }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-focus-window";
    version = "0.1.0";
    src = ../res/lumen-desktop/focus-window;
    cargoLock.lockFile = ../res/lumen-desktop/focus-window/Cargo.lock;
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-focus-window";
  inherit binary;
  runtimeInputs = [ pkgs.sway ];
}
