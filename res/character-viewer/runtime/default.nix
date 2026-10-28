{ pkgs, fontPackage ? pkgs.noto-fonts-color-emoji, fontFile ? "${fontPackage}/share/fonts/noto/NotoColorEmoji.ttf", fontFamily ? "Noto Color Emoji" }:
let
  source = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: !(pkgs.lib.hasSuffix ".nu" path);
  };
  bridge = builtins.derivation {
    name = "character-viewer-input-bridge-0.1.0";
    system = pkgs.stdenv.hostPlatform.system;
    builder = "${pkgs.nushell}/bin/nu";
    args = [ "${./bridge-build.nu}" "${source}/input-method.c" "${source}/protocols/input-method-unstable-v2.xml" ];
    PATH = pkgs.lib.makeBinPath [ pkgs.wayland-scanner pkgs.pkg-config pkgs.stdenv.cc pkgs.coreutils ];
    PKG_CONFIG_PATH =
      let
        inputs = map pkgs.lib.getDev [
          pkgs.gtk3 pkgs.wayland pkgs.libxkbcommon pkgs.pango pkgs.cairo
          pkgs.gdk-pixbuf pkgs.atk pkgs.glib pkgs.harfbuzz pkgs.fontconfig
          pkgs.freetype pkgs.libepoxy pkgs.fribidi pkgs.pixman pkgs.zlib
        ];
      in pkgs.lib.makeSearchPath "lib/pkgconfig" inputs
        + ":" + pkgs.lib.makeSearchPath "share/pkgconfig" inputs;
  };
  binary = pkgs.rustPlatform.buildRustPackage {
    pname = "character-viewer-runtime";
    version = "0.1.0";
    src = source;
    cargoRoot = "runtime";
    buildAndTestSubdir = "runtime";
    cargoLock.lockFile = ./Cargo.lock;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.gtk3 ];
  };
in builtins.derivation {
  name = "lumen-character-viewer-0.1.0";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [
    "${./assemble.nu}"
    "${binary}/bin/character-viewer-runtime"
    "${source}/icons"
    "${source}/emoji-test.txt"
    "${source}/UNICODE-LICENSE.txt"
    "${bridge}/lib/libcharacter-input.so"
    "${../../lumen-desktop/appearance/palette.json}"
    "${pkgs.fontconfig.lib}/lib/libfontconfig.so.1"
    fontFile
    fontFamily
    "${pkgs.nushell}/bin/nu"
    "${pkgs.lib.makeBinPath [ pkgs.sway pkgs.wtype pkgs.wl-clipboard pkgs.libnotify ]}"
  ];
  PATH = pkgs.lib.makeBinPath [ pkgs.coreutils ];
}
