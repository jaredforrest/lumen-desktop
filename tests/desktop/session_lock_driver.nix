{ pkgs }:
pkgs.runCommand "lumen-test-session-lock" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.wayland ];
} ''
  protocol=${pkgs.wayland-protocols}/share/wayland-protocols/staging/ext-session-lock/ext-session-lock-v1.xml
  wayland-scanner client-header "$protocol" ext-session-lock-v1-client-protocol.h
  wayland-scanner private-code "$protocol" protocol.c
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror -I. ${./session_lock_driver.c} protocol.c $(pkg-config --cflags --libs wayland-client) -o "$out/bin/lumen-test-session-lock"
''
