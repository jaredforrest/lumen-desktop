{ pkgs }:
let
  source = pkgs.writeText "desktop-defaults.nix" "{}";
  defaults = pkgs.writeText "standalone-app-defaults.json" (builtins.toJSON (import ../config/application-profiles.nix));
  # Preserve relative imports without depending on unrelated package recipes.
  schemaSource = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      ./desktop-settings-schema.nix
      ./shortcut-preferences-type.nix
      ../config/application-profiles.nix
    ];
  };
  schema = "${schemaSource}/packages/desktop-settings-schema.nix";
  builder = pkgs.writeText "desktop-settings-builder.nix" ''
    { source }:
    let pkgs = import ${pkgs.path} { system = "${pkgs.stdenv.hostPlatform.system}"; };
    in pkgs.writeTextDir "settings.json" (builtins.toJSON (import ${schema} {
      inherit (pkgs) lib; defaults = builtins.fromJSON (builtins.readFile ${defaults}); values = import (builtins.toPath source);
    }))
  '';
  runtime = pkgs.callPackage ./settings-runtime.nix { };
in pkgs.writeTextFile {
  name = "lumen-nix-settings";
  destination = "/bin/lumen-nix-settings";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.nix pkgs.procps pkgs.git pkgs.coreutils ])} | split row ':') ++ $env.PATH)
      $env.LUMEN_NIX_SETTINGS_SOURCE = "${source}"
      $env.LUMEN_NIX_SETTINGS_BUILDER = "${builder}"
      exec ${runtime}/bin/lumen-nix-settings ...$args
    }
  '';
}
