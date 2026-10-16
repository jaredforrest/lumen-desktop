{ pkgs }:
let
  python = pkgs.python3.withPackages (ps: [ ps.dbus-python ps.pygobject3 ps.icalendar ps.recurring-ical-events ps.astral ps.pillow ]);
  typelibs = pkgs.lib.makeSearchPath "lib/girepository-1.0" [ pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ];
in pkgs.writeShellScriptBin "desktop-test-python" ''
  export GI_TYPELIB_PATH=${typelibs}
  export DESKTOP_TEST_PINNED_RUNTIME=1
  exec ${python}/bin/python3 "$@"
''
