{ pkgs }:
let
  fontConfig = pkgs.makeFontsConf {
    fontDirectories = [
      "${pkgs.callPackage ../../packages/interlude.nix {}}/share/fonts"
      "${pkgs.dejavu_fonts}/share/fonts"
      "${pkgs.freefont_ttf}/share/fonts"
      "${pkgs.noto-fonts-color-emoji}/share/fonts"
    ];
  };
in pkgs.stdenv.mkDerivation {
  pname = "lumen-typography-probe";
  version = "1";
  dontUnpack = true;
  nativeBuildInputs = [ pkgs.pkg-config pkgs.makeWrapper ];
  buildInputs = [ pkgs.qt6.qtbase ];
  dontWrapQtApps = true;
  buildPhase = ''
    $CXX -std=c++17 -Wall -Wextra -Werror ${./typography_probe.cpp} -o typography-probe $(pkg-config --cflags --libs Qt6Gui)
  '';
  installPhase = ''
    mkdir -p $out/bin $out/libexec
    cp typography-probe $out/libexec/
    makeWrapper $out/libexec/typography-probe $out/bin/typography-probe \
      --set FONTCONFIG_FILE ${fontConfig} \
      --set QT_QPA_PLATFORM offscreen \
      --set QT_QPA_PLATFORMTHEME generic \
      --set QT_PLUGIN_PATH ${pkgs.qt6.qtbase}/lib/qt-6/plugins
  '';
}
