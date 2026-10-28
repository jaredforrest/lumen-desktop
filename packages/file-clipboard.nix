{ pkgs }:
builtins.derivation {
  name = "lumen-file-clipboard-1.0";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./wayland-clipboard-build.nu}" "file" "${../res/lumen-desktop/file-clipboard.c}" "${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.wayland-scanner pkgs.pkg-config pkgs.stdenv.cc pkgs.coreutils ];
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" [ pkgs.wayland.dev ];
}
