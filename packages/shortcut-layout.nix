{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-shortcut-layout";
  version = "0.1.0";
  src = ../res/lumen-desktop/shortcut-layout;
  cargoLock.lockFile = ../res/lumen-desktop/shortcut-layout/Cargo.lock;
  buildInputs = [ pkgs.libxkbcommon ];
  env.LUMEN_XKB_ROOT = "${pkgs.xkeyboard_config}/share/X11/xkb";
}
