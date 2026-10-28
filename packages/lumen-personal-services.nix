{ pkgs, timeZone ? "Europe/London" }:
let
  calendarRuntime = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-calendar-runtime";
    version = "0.1.0";
    src = ../res/lumen-desktop/shell/calendar-runtime;
    cargoLock.lockFile = ../res/lumen-desktop/shell/calendar-runtime/Cargo.lock;
    cargoBuildFlags = [ "--features" "eds" ];
    cargoTestFlags = [ "--features" "eds" ];
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.evolution-data-server pkgs.gnome-online-accounts pkgs.libical pkgs.glib-networking ];
  };
  setupRuntime = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-icloud-setup";
    version = "0.1.0";
    src = ../res/lumen-desktop/icloud-setup;
    cargoLock.lockFile = ../res/lumen-desktop/icloud-setup/Cargo.lock;
  };
  setup = pkgs.writeTextFile {
    name = "lumen-icloud-setup";
    destination = "/bin/lumen-icloud-setup";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def --wrapped main [...args] {
        $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.vdirsyncer pkgs.libsecret ])} | split row ':') ++ $env.PATH)
        exec ${setupRuntime}/bin/lumen-icloud-setup ...$args
      }
    '';
  };
  calendar = pkgs.writeTextFile {
    name = "lumen-calendar";
    destination = "/bin/lumen-calendar";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def --wrapped main [...args] {
        $env.PATH = (["${calendarRuntime}/bin"] ++ (${builtins.toJSON (pkgs.lib.makeBinPath [ googleSync setup pkgs.vdirsyncer pkgs.libsecret ])} | split row ':') ++ $env.PATH)
        $env.CALENDAR_DESKTOP_PREFIX = "lumen"
        $env.CALENDAR_UID_SUFFIX = "@lumen-desktop.local"
        $env.LUMEN_CALENDAR_TIMEZONE = ${builtins.toJSON timeZone}
        exec ${calendarRuntime}/bin/desktop-calendar ...$args
      }
    '';
  };
  googleSync = pkgs.writeTextFile {
    name = "lumen-google-calendar-sync";
    destination = "/bin/lumen-google-calendar-sync";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def --wrapped main [...args] {
        $env.LUMEN_CALENDAR_TIMEZONE = ${builtins.toJSON timeZone}
        exec ${calendarRuntime}/bin/desktop-google-calendar-sync ...$args
      }
    '';
  };
  googleSetup = pkgs.writeTextFile {
    name = "lumen-google-calendar-setup";
    destination = "/bin/lumen-google-calendar-setup";
    executable = true;
    text = ''
      #!${pkgs.nushell}/bin/nu
      def main [] {
        exec ${pkgs.gnome-online-accounts-gtk}/bin/gnome-online-accounts-gtk
      }
    '';
  };
in pkgs.symlinkJoin {
  name = "lumen-personal-services";
  paths = [ calendar googleSync setup googleSetup ];
  passthru = { inherit calendarRuntime; };
}
