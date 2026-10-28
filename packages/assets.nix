{ pkgs, icons ? null, wallpaper ? ../res/lumen-desktop/shell/wallpaper.png, brandMark ? null }:
assert builtins.pathExists wallpaper;
assert icons == null || builtins.pathExists icons;
assert brandMark == null || builtins.pathExists brandMark;
builtins.derivation {
  name = "lumen-desktop-assets";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./assets.nu}" "${../res/lumen-desktop}" "${wallpaper}" (if brandMark == null then "" else "${brandMark}") (if icons == null then "" else "${icons}") ];
  PATH = pkgs.lib.makeBinPath [ pkgs.coreutils ];
}
