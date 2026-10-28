{ pkgs }:
let
  exporter = pkgs.callPackage ./menu-export.nix { };
  bridge = pkgs.callPackage ./menu-bridge.nix { };
in builtins.derivation {
  name = "lumen-global-menu-runtime-1.0";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./global-menu-runtime.nu}" "${../res/lumen-desktop/global-menu/menu-adapter.cjs}" "${exporter}/bin/lumen-menu-export" "${bridge}/bin/lumen-menu-bridge" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.coreutils ];
}
