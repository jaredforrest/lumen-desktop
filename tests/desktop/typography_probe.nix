{ pkgs }:
let
  fontConfig = (import ../../packages/font-configuration.nix { inherit pkgs; }) {
    fontDirectories = [
      "${pkgs.callPackage ../../packages/interlude.nix { }}/share/fonts"
      "${pkgs.dejavu_fonts}/share/fonts"
      "${pkgs.freefont_ttf}/share/fonts"
      "${pkgs.noto-fonts-color-emoji}/share/fonts"
    ];
  };
in (import ../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-typography-probe";
  version = "0.1.0";
  src = ./typography-probe-rust;
  cargoLock.lockFile = ./typography-probe-rust/Cargo.lock;
  LUMEN_TYPOGRAPHY_QML = "${pkgs.qt6.qtdeclarative}/bin/qml";
  LUMEN_TYPOGRAPHY_QT_VERSION = pkgs.qt6.qtbase.version;
  LUMEN_TYPOGRAPHY_FONTCONFIG = "${fontConfig}";
}
