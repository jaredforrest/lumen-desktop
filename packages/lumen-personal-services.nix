{ pkgs, timeZone ? "Europe/London" }:
let
  assets = ../res/lumen-desktop/shell;
  calendarPython = (import ./python-runtime.nix { inherit pkgs; }).withPackages (ps: [ ps.icalendar ps.recurring-ical-events ps.pygobject3 ]);
  calendar = pkgs.stdenvNoCC.mkDerivation {
    name = "lumen-calendar-runtime";
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.makeWrapper pkgs.wrapGAppsHook3 pkgs.gobject-introspection ];
    buildInputs = [ pkgs.evolution-data-server pkgs.gnome-online-accounts pkgs.libical pkgs.glib-networking ];
    installPhase = ''
      mkdir -p $out/bin
      makeWrapper ${calendarPython}/bin/python3 $out/bin/lumen-calendar-python
      makeWrapper ${calendarPython}/bin/python3 $out/bin/lumen-calendar \
        --add-flags ${assets}/calendar_store.py \
        --set LUMEN_CALENDAR_TIMEZONE ${pkgs.lib.escapeShellArg timeZone} \
        --prefix PATH : "$out/bin:${pkgs.lib.makeBinPath [ setup pkgs.vdirsyncer pkgs.libsecret ]}"
      makeWrapper ${calendarPython}/bin/python3 $out/bin/lumen-google-calendar-sync \
        --add-flags ${assets}/google_calendar_eds.py \
        --set LUMEN_CALENDAR_TIMEZONE ${pkgs.lib.escapeShellArg timeZone}
    '';
  };
  setup = pkgs.writeShellApplication {
    name = "lumen-icloud-setup";
    runtimeInputs = [ pkgs.vdirsyncer pkgs.libsecret ];
    text = ''
      exec ${pkgs.python3}/bin/python3 ${assets}/icloud_setup.py "$@"
    '';
  };
  googleSetup = pkgs.writeShellApplication {
    name = "lumen-google-calendar-setup";
    runtimeInputs = [ pkgs.gnome-online-accounts-gtk ];
    text = ''
      exec gnome-online-accounts-gtk
    '';
  };
in pkgs.symlinkJoin {
  name = "lumen-personal-services";
  paths = [ calendar setup googleSetup ];
}
