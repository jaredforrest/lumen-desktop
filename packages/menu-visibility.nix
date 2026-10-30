{ pkgs }:
let
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-menu-visibility";
    version = "0.1.0";
    src = ../res/lumen-desktop/menu-visibility;
    cargoLock.lockFile = ../res/lumen-desktop/menu-visibility/Cargo.lock;
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-menu-visibility";
  inherit binary;
  runtimeInputs = [ pkgs.sway pkgs.coreutils ];
}
