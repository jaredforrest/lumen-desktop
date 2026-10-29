{ pkgs }:
builtins.derivation {
  name = "lumen-clipboard-observer-1";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./wayland-clipboard-build.nu}" "observer" "${../res/lumen-desktop/runtime/clipboard_observer.c}" "${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.wayland-scanner pkgs.pkg-config pkgs.stdenv.cc pkgs.coreutils ];
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" [ pkgs.wayland.dev ];
}
