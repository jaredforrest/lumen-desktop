{ pkgs }:
let
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-desktop-files";
    version = "0.1.0";
    src = ../res/lumen-desktop/desktop-files;
    cargoLock.lockFile = ../res/lumen-desktop/desktop-files/Cargo.lock;
  };
in (import ./nu-wrapped-rust.nix { inherit pkgs; }) {
  name = "lumen-desktop-files";
  inherit binary;
  runtimeInputs = [ pkgs.xdg-user-dirs ];
}
