{ pkgs }:
let
  python = (import ../../packages/python-runtime.nix { inherit pkgs; }).withPackages (ps: [ ps.dbus-python ps.pygobject3 ps.icalendar ps.recurring-ical-events ps.astral ps.pillow ]);
  typelibs = pkgs.lib.makeSearchPath "lib/girepository-1.0" (map pkgs.lib.getLib [ pkgs.gobject-introspection pkgs.pango pkgs.gdk-pixbuf pkgs.atk pkgs.harfbuzz pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ]);
in pkgs.writeShellScriptBin "desktop-test-python" ''
  export GI_TYPELIB_PATH=${typelibs}
  export XDG_DATA_DIRS=${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:"''${XDG_DATA_DIRS:-}"
  export DESKTOP_TEST_PINNED_RUNTIME=1
  export TZDIR=${pkgs.tzdata}/share/zoneinfo
  export LUMEN_TEST_AT_SPI=${pkgs.at-spi2-core}
  exec ${python}/bin/python3 "$@"
''
