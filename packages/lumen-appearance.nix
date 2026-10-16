{ pkgs, nixSettings }:
let
  settingsModule = pkgs.writeTextDir "nix_settings.py" (builtins.readFile ../res/lumen-desktop/shell/nix_settings.py);
  python=pkgs.python3.withPackages (p: [ p.astral ]);
in pkgs.writeShellApplication {
  name = "lumen-appearance";
  runtimeInputs = [ pkgs.glib pkgs.sway nixSettings ];
  text = ''
    export PYTHONPATH="${settingsModule}:''${PYTHONPATH:-}"
    export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:''${XDG_DATA_DIRS:-}"
    exec ${python}/bin/python3 ${../res/lumen-desktop/appearance}/appearance.py "$@"
  '';
}
