{ pkgs, package, app }:
let
  appearanceAdapter = pkgs.callPackage ./native-window-preferences-rust.nix { };
  rewriteDesktopEntries = pkgs.writeText "lumen-native-window-desktop-entries.nu" ''
    def main [output: string, original: string, app: string] {
      for entry in (glob $"($output)/share/applications/*.desktop") {
        let source = (^${pkgs.coreutils}/bin/readlink -f $entry | str trim)
        ^${pkgs.coreutils}/bin/cp --remove-destination $source $entry
        ^${pkgs.coreutils}/bin/chmod u+w $entry
        let content = (open --raw $entry | str replace --all $"($original)/bin/" $"($output)/bin/")
        let updated = if $app == "gimp" {
          $content | str replace --all '/bin/gimp-3.2' '/bin/gimp'
        } else { $content }
        $updated | save --raw --force $entry
      }
    }
  '';
in pkgs.symlinkJoin {
  name = "${package.name}-native-window-frame";
  paths = [ package ] ++ pkgs.lib.optional (package ? man) package.man;
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram "$out/bin/${package.meta.mainProgram}" --run '${appearanceAdapter}/bin/lumen-native-window-preferences ${app} "$@"'
    ${pkgs.nushell}/bin/nu ${rewriteDesktopEntries} "$out" '${package}' '${app}'
  '';
  meta = package.meta // { outputsToInstall = [ "out" ]; };
}
