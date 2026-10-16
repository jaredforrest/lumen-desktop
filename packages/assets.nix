{ pkgs, icons ? null, wallpaper ? ../res/lumen-desktop/shell/wallpaper.png, brandMark ? null }:
assert builtins.pathExists wallpaper;
assert icons == null || builtins.pathExists icons;
assert brandMark == null || builtins.pathExists brandMark;
pkgs.runCommand "lumen-desktop-assets" {} ''
  cp -r ${../res/lumen-desktop} "$out"
  chmod -R u+w "$out"
  cp --remove-destination ${wallpaper} "$out/shell/wallpaper.png"
  ${pkgs.lib.optionalString (brandMark != null) ''
    cp --remove-destination ${brandMark} "$out/shell/icons/lumen.svg"
  ''}
  ${pkgs.lib.optionalString (icons != null) ''
    cp -r ${icons}/. "$out/shell/reference-icons/"
  ''}
''
