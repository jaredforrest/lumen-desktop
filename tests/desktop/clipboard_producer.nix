{ pkgs }:
pkgs.runCommand "lumen-test-clipboard-producer" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
} ''
  wayland-scanner client-header ${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml wlr-data-control-client-protocol.h
  wayland-scanner private-code ${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-data-control-unstable-v1.xml protocol.c
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror ${./clipboard_empty_provider.c} $(pkg-config --cflags --libs wayland-server) -o "$out/bin/lumen-test-clipboard-empty-provider"
  cc -Wall -Wextra -Werror -I. ${./clipboard_producer.c} protocol.c $(pkg-config --cflags --libs wayland-client) -o "$out/bin/lumen-test-clipboard-producer"
''
