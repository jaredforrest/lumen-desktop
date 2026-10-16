{ pkgs, package, app }:
let
  appearanceAdapter = pkgs.runCommand "native-window-appearance-adapter" {} ''
    mkdir -p "$out"
    cp ${../res/lumen-desktop/native_window_preferences.py} "$out/native_window_preferences.py"
    cp ${../res/lumen-desktop/brave-appearance.json} "$out/brave-appearance.json"
  '';
in pkgs.symlinkJoin {
  name = "${package.name}-native-window-frame";
  paths = [ package ] ++ pkgs.lib.optional (package ? man) package.man;
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram "$out/bin/${package.meta.mainProgram}" --run '${pkgs.python3}/bin/python3 ${appearanceAdapter}/native_window_preferences.py ${app} "$@"'
    # Some upstream desktop entries use an absolute executable path.
    for entry in "$out"/share/applications/*.desktop; do
      [ -e "$entry" ] || continue
      cp --remove-destination "$(readlink -f "$entry")" "$entry"
      substituteInPlace "$entry" --replace-quiet '${package}/bin/' "$out/bin/"
      ${pkgs.lib.optionalString (app == "gimp") ''
        substituteInPlace "$entry" --replace-quiet '/bin/gimp-3.2' '/bin/gimp'
      ''}
    done
  '';
  meta = package.meta // { outputsToInstall = [ "out" ]; };
}
