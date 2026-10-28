{ pkgs, profileBridge }:
let
  bridge = profileBridge;
  rewriteDesktopEntries = pkgs.writeText "lumen-prism-desktop-entries.nu" ''
    def main [output: string] {
      for desktop in (glob $"($output)/share/applications/*Prism*.desktop") {
        let original = (^${pkgs.coreutils}/bin/readlink -f $desktop | str trim)
        ^${pkgs.coreutils}/bin/cp --remove-destination $original $desktop
        ^${pkgs.coreutils}/bin/chmod u+w $desktop
        let original_content = (open --raw $desktop)
        if not ($original_content | str contains 'Exec=prismlauncher') {
          error make {msg: $"Missing Exec=prismlauncher in ($desktop)"}
        }
        let content = ($original_content | str replace --all 'Exec=prismlauncher' $"Exec=($output)/bin/prismlauncher")
        $content | save --raw --force $desktop
      }
    }
  '';
in pkgs.symlinkJoin {
  name = "prismlauncher-nix-profile";
  paths = [ pkgs.prismlauncher ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    rm $out/bin/prismlauncher
    makeWrapper ${bridge}/bin/lumen-native-profile $out/bin/prismlauncher \
      --add-flags "prism ${pkgs.prismlauncher}/bin/prismlauncher"
    ${pkgs.nushell}/bin/nu ${rewriteDesktopEntries} "$out"
  '';
}
