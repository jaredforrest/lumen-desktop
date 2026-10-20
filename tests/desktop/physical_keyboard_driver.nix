{ pkgs }:
pkgs.runCommand "lumen-test-physical-keyboard" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland pkgs.libxkbcommon ];
} ''
  protocol=${pkgs.wtype.src}/protocol/virtual-keyboard-unstable-v1.xml
  wayland-scanner client-header "$protocol" virtual-keyboard-unstable-v1-client-protocol.h
  wayland-scanner private-code "$protocol" protocol.c
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror -I. ${./application_fixture/keyboard.c} protocol.c $(pkg-config --cflags --libs wayland-client xkbcommon) -o "$out/bin/lumen-test-physical-keyboard"
''
