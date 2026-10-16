{ pkgs, rofiPackage ? pkgs.rofi, characterViewer ? import ../res/character-viewer { inherit pkgs; } }:
 pkgs.writeShellApplication {
  name = "lumen-utilities";
  runtimeInputs = [ pkgs.python3 pkgs.sway rofiPackage characterViewer
    pkgs.wl-clipboard pkgs.grim pkgs.slurp pkgs.satty pkgs.wf-recorder
    pkgs.systemd pkgs.libnotify ];
  text = ''
    export LUMEN_UTILITIES_WRAPPER="$0"
    exec python3 ${../res/utilities/conveniences.py} "$@"
  '';
}
