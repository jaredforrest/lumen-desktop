{ pkgs, bluetoothPower, powerStatus, networkConnectivity }:
let
  vaultClient = import ./wifi-vault-rbw.nix { inherit pkgs; };
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-system-controls";
    version = "0.1.0";
    src = ../res/lumen-desktop/system-controls;
    cargoLock.lockFile = ../res/lumen-desktop/system-controls/Cargo.lock;
    nativeCheckInputs = [ pkgs.nushell ];
  };
in pkgs.writeTextFile {
  passthru = { inherit binary vaultClient; };
  name = "lumen-system-controls";
  destination = "/bin/lumen-system-controls";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.systemd pkgs.networkmanager vaultClient pkgs.foot pkgs.sway pkgs.brightnessctl pkgs.ddcutil bluetoothPower powerStatus networkConnectivity ])} | split row ':') ++ $env.PATH)
      exec ${binary}/bin/lumen-system-controls ...$args
    }
  '';
}
