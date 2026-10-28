{ pkgs }:
let
  agent = import ../../packages/lumen-authorization-agent.nix { inherit pkgs; };
  schemas = pkgs.gsettings-desktop-schemas;
in pkgs.runCommand "polkit-session-appearance-test" {
  nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc pkgs.xvfb-run ];
  buildInputs = [ pkgs.gtk3 ];
} ''
  rustc --edition 2024 -D warnings \
    -L native=${pkgs.gtk3}/lib -L native=${pkgs.glib.out}/lib \
    ${./polkit-appearance.rs} -o fixture
  export HOME="$TMPDIR/home"
  mkdir -p "$HOME"
  export GSETTINGS_BACKEND=memory NO_AT_BRIDGE=1
  export GSETTINGS_SCHEMA_DIR=${schemas}/share/gsettings-schemas/${schemas.name}/glib-2.0/schemas
  export GTK_MODULES=${agent.bridge}/lib/gtk-3.0/modules/libsession-appearance.so
  xvfb-run -a ./fixture
  touch $out
''
