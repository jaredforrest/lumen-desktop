{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-installed-smoke-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postPatch = ''
    cp ${../browser-workflows-rust/src/keyboard.rs} src/keyboard.rs
  '';
  LUMEN_SMOKE_KEYBOARD = "${import ../physical_keyboard_driver.nix { inherit pkgs; }}/bin/lumen-test-physical-keyboard";
  LUMEN_SMOKE_POINTER = "${import ../pointer_driver.nix { inherit pkgs; }}/bin/wlrctl";
}
