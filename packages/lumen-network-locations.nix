{ pkgs, menuRuntime, assets ? ../res/lumen-desktop, providerPackage ? null }:
let
  bridge = (import ./lumen-authorization-agent.nix { inherit pkgs; }).bridge;
in pkgs.writeShellScriptBin "lumen-network-locations" ''
  export GTK_MODULES="${bridge}/lib/gtk-3.0/modules/libsession-appearance.so''${GTK_MODULES:+:$GTK_MODULES}"
  ${pkgs.lib.optionalString (providerPackage != null) ''export GIO_EXTRA_MODULES="${providerPackage}/lib/gio/modules''${GIO_EXTRA_MODULES:+:$GIO_EXTRA_MODULES}"''}
  exec ${menuRuntime}/bin/global-menu-python ${assets}/shell/network_locations.py "$@"
''
