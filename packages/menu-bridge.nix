{ pkgs }:
let
  exporter = pkgs.callPackage ./menu-export.nix { };
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-menu-bridge";
    version = "0.1.0";
    src = ../res/lumen-desktop/global-menu/bridge;
    cargoLock.lockFile = ../res/lumen-desktop/global-menu/bridge/Cargo.lock;
    nativeCheckInputs = [ pkgs.dbus pkgs.coreutils ];
    LUMEN_TEST_DBUS_CONFIG = "${pkgs.dbus}/share/dbus-1/session.conf";
    LUMEN_TEST_EXPORT_BIN = "${exporter}/bin/lumen-menu-export";
  };
in pkgs.writeTextFile {
  name = "lumen-menu-bridge";
  destination = "/bin/lumen-menu-bridge";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.sway pkgs.coreutils ])} | split row ':') ++ $env.PATH)
      exec ${binary}/bin/lumen-menu-bridge ...$args
    }
  '';
}
