{ pkgs }:
let
  source = pkgs.writeText "desktop-defaults.nix" "{}";
  defaults = pkgs.writeText "standalone-app-defaults.json" (builtins.toJSON (import ../config/application-profiles.nix));
  schema = ./desktop-settings-schema.nix;
  builder = pkgs.writeText "desktop-settings-builder.nix" ''
    { source }:
    let pkgs = import ${pkgs.path} { system = "${pkgs.stdenv.hostPlatform.system}"; };
    in pkgs.writeTextDir "settings.json" (builtins.toJSON (import ${schema} {
      inherit (pkgs) lib; defaults = builtins.fromJSON (builtins.readFile ${defaults}); values = import (builtins.toPath source);
    }))
  '';
in pkgs.writeShellApplication {
  name = "lumen-nix-settings";
  runtimeInputs = [ pkgs.nix pkgs.python3 pkgs.procps pkgs.git ];
  text = ''
    export LUMEN_NIX_SETTINGS_SOURCE=${source}
    export LUMEN_NIX_SETTINGS_BUILDER=${builder}
    exec python3 ${../res/lumen-desktop/shell/nix_settings.py} "$@"
  '';
}
