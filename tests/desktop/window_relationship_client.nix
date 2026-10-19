{ pkgs }:
pkgs.runCommand "lumen-test-window-relationships" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
} ''
  wayland-scanner client-header ${pkgs.wayland-protocols}/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml xdg-shell-client-protocol.h
  wayland-scanner private-code ${pkgs.wayland-protocols}/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml shell.c
  wayland-scanner client-header ${pkgs.wayland-protocols}/share/wayland-protocols/staging/xdg-dialog/xdg-dialog-v1.xml xdg-dialog-v1-client-protocol.h
  wayland-scanner private-code ${pkgs.wayland-protocols}/share/wayland-protocols/staging/xdg-dialog/xdg-dialog-v1.xml dialog.c
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror -I. ${./window_relationship_client.c} shell.c dialog.c $(pkg-config --cflags --libs wayland-client) -o "$out/bin/lumen-test-window-relationships"
''
