{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "lumen-file-clipboard";
  version = "1.0";
  src = ../res/lumen-desktop/file-clipboard.c;
  dontUnpack = true;
  nativeBuildInputs = [ pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
  buildPhase = ''
    protocol=${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml
    wayland-scanner client-header "$protocol" data-control.h
    wayland-scanner private-code "$protocol" data-control.c
    $CC -O2 -Wall -I. $src data-control.c $(pkg-config --cflags --libs wayland-client) -o lumen-file-clipboard
  '';
  installPhase = ''
    install -Dm755 lumen-file-clipboard $out/bin/lumen-file-clipboard
  '';
}
