{ pkgs, profileBridge }:
let bridge = profileBridge;
in pkgs.symlinkJoin {
  name = "prismlauncher-nix-profile";
  paths = [ pkgs.prismlauncher ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    rm $out/bin/prismlauncher
    makeWrapper ${bridge}/bin/lumen-native-profile $out/bin/prismlauncher \
      --add-flags "prism ${pkgs.prismlauncher}/bin/prismlauncher"
    for desktop in $out/share/applications/*Prism*.desktop; do
      original=$(readlink -f "$desktop")
      rm "$desktop"
      cp "$original" "$desktop"
      substituteInPlace "$desktop" --replace-fail "Exec=prismlauncher" "Exec=$out/bin/prismlauncher"
    done
  '';
}
