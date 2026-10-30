{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-switcher-fixture";
  version = "0.1.0";
  src = ./.;
  env.LUMEN_LAYOUT_KEYMAP_US = pkgs.runCommand "lumen-layout-fixture-us" {} ''
    ${pkgs.libxkbcommon}/bin/xkbcli compile-keymap --include ${pkgs.xkeyboard_config}/share/X11/xkb --layout us --format 1 > $out
  '';
  env.LUMEN_LAYOUT_KEYMAP_DE = pkgs.runCommand "lumen-layout-fixture-de" {} ''
    ${pkgs.libxkbcommon}/bin/xkbcli compile-keymap --include ${pkgs.xkeyboard_config}/share/X11/xkb --layout de --format 1 > $out
  '';
  cargoLock.lockFile = ./Cargo.lock;
}
