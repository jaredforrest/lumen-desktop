{ pkgs }:
pkgs.stdenv.mkDerivation {
  pname = "lumen-clipboard-observer";
  version = "1";
  dontUnpack = true;
  nativeBuildInputs = [ pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
  buildPhase = ''
    runHook preBuild
    wayland-scanner client-header ${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml wlr-data-control-client-protocol.h
    wayland-scanner private-code ${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml protocol.c
    $CC -std=c11 -Wall -Wextra -Werror -I. ${../res/lumen-desktop/runtime/clipboard_observer.c} protocol.c $(pkg-config --cflags --libs wayland-client) -o lumen-clipboard-observer
    runHook postBuild
  '';
  installPhase = ''
    mkdir -p "$out/bin"
    cp lumen-clipboard-observer "$out/bin/"
  '';
}
