{ pkgs, nixSettings, searchHistory }:
let
  runtime = pkgs.callPackage ./appearance-rust.nix { };
in pkgs.writeTextFile {
  name = "lumen-appearance";
  destination = "/bin/lumen-appearance";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      $env.PATH = ((${builtins.toJSON (pkgs.lib.makeBinPath [ pkgs.glib pkgs.sway pkgs.coreutils nixSettings searchHistory ])} | split row ':') ++ $env.PATH)
      $env.XDG_DATA_DIRS = $"${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:($env.XDG_DATA_DIRS? | default "")"
      if ($args.0? | default "") == "--print-env0" {
        exec ${pkgs.coreutils}/bin/env -0
      }
      exec ${runtime}/bin/lumen-appearance ...$args
    }
  '';
}
