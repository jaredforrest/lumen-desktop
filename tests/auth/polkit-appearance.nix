{ pkgs }:
let
  agent = import ../../packages/lumen-authorization-agent.nix { inherit pkgs; };
  schemas = pkgs.gsettings-desktop-schemas;
in pkgs.runCommand "polkit-session-appearance-test" {
  nativeBuildInputs = [ pkgs.stdenv.cc pkgs.pkg-config pkgs.xvfb-run ];
  buildInputs = [ pkgs.gtk3 ];
} ''
  $CC -Wall -Wextra -Werror $(pkg-config --cflags gtk+-3.0) \
    ${./polkit-appearance.c} $(pkg-config --libs gtk+-3.0) -o fixture
  export HOME="$TMPDIR/home"
  mkdir -p "$HOME"
  export GSETTINGS_BACKEND=memory NO_AT_BRIDGE=1
  export GSETTINGS_SCHEMA_DIR=${schemas}/share/gsettings-schemas/${schemas.name}/glib-2.0/schemas
  export GTK_MODULES=${agent.bridge}/lib/gtk-3.0/modules/libsession-appearance.so
  xvfb-run -a ./fixture
  touch $out
''
