{ pkgs, providerPackage ? null }:
let
  bridge = (import ./lumen-authorization-agent.nix { inherit pkgs; }).bridge;
  binary = (import ./owned-rust.nix { inherit pkgs; }) {
    pname = "lumen-network-locations";
    version = "0.1.0";
    src = ../res/lumen-desktop/network-locations;
    cargoLock.lockFile = ../res/lumen-desktop/network-locations/Cargo.lock;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.gtk3 ];
  };
in pkgs.writeTextFile {
  name = "lumen-network-locations";
  destination = "/bin/lumen-network-locations";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.GTK_MODULES = $"${bridge}/lib/gtk-3.0/modules/libsession-appearance.so:($env.GTK_MODULES? | default "")"
      ${if providerPackage == null then "" else ''$env.GIO_EXTRA_MODULES = $"${providerPackage}/lib/gio/modules:($env.GIO_EXTRA_MODULES? | default "")"''}
      exec ${binary}/bin/lumen-network-locations ...$args
    }
  '';
}
