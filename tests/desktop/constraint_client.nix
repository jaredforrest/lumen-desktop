{ pkgs }:
pkgs.runCommand "lumen-test-constraint-client" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
} ''
  wayland-scanner client-header ${pkgs.wayland-protocols}/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml xdg-shell-client-protocol.h
  wayland-scanner private-code ${pkgs.wayland-protocols}/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml xdg.c
  wayland-scanner client-header ${pkgs.wayland-protocols}/share/wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml pointer-constraints-unstable-v1-client-protocol.h
  wayland-scanner private-code ${pkgs.wayland-protocols}/share/wayland-protocols/unstable/pointer-constraints/pointer-constraints-unstable-v1.xml constraints.c
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror -I. ${./constraint_client.c} xdg.c constraints.c $(pkg-config --cflags --libs wayland-client) -o "$out/bin/lumen-test-constraint-client"
''
