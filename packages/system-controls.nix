{ pkgs, bluetoothPower, powerStatus, networkConnectivity }:
let
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "lumen-system-controls";
    version = "0.1.0";
    src = ../res/lumen-desktop/system-controls;
    cargoLock.lockFile = ../res/lumen-desktop/system-controls/Cargo.lock;
  };
in pkgs.writeTextFile {
  name = "lumen-system-controls";
  destination = "/bin/lumen-system-controls";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.systemd pkgs.sway pkgs.ddcutil bluetoothPower powerStatus networkConnectivity ])} | split row ':') ++ $env.PATH)
      exec ${binary}/bin/lumen-system-controls ...$args
    }
  '';
}
