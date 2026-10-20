{ pkgs }:
pkgs.runCommand "lumen-test-xwayland-relationships" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config ];
  buildInputs = [ pkgs.libxcb ];
} ''
  mkdir -p "$out/bin"
  cc -Wall -Wextra -Werror ${./xwayland_relationship_client.c} $(pkg-config --cflags --libs xcb) -o "$out/bin/lumen-test-xwayland-relationships"
''
