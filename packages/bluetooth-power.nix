{ pkgs }:
let
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-bluetooth-power";
    version = "0.1.0";
    src = ../res/lumen-desktop/bluetooth-power;
    cargoLock.lockFile = ../res/lumen-desktop/bluetooth-power/Cargo.lock;
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-bluetooth-power";
  inherit binary;
  runtimeInputs = [ pkgs.util-linux ];
}
