{ pkgs }:
let
  font = pkgs.callPackage ./interlude.nix {};
  skin = pkgs.runCommand "localsend-desktop-presentation" {} ''
    mkdir -p $out
    cp ${../res/lumen-desktop/style_localsend.py} $out/style_localsend.py
    cp ${../res/lumen-desktop/localsend-theme.dart} $out/localsend-theme.dart
    cp ${../res/lumen-desktop/localsend_sidebar.dart} $out/localsend_sidebar.dart
  '';
in pkgs.localsend.overrideAttrs (old: {
  nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ pkgs.python3 ];
  postPatch = (old.postPatch or "") + ''
    ${pkgs.python3}/bin/python3 ${skin}/style_localsend.py . \
      ${font}/share/fonts/truetype/InterludeVariable.ttf
  '';
})
