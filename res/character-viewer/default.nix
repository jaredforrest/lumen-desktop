{ pkgs }:
let
  python = pkgs.python3.withPackages (p: [ p.pygobject3 ]);
  font = pkgs.noto-fonts-color-emoji;
in pkgs.stdenv.mkDerivation {
  pname = "lumen-character-viewer";
  version = "1.0";
  src = ./.;
  nativeBuildInputs = [ pkgs.wrapGAppsHook3 pkgs.makeWrapper pkgs.gobject-introspection pkgs.pkg-config pkgs.wayland-scanner ];
  buildInputs = [ pkgs.gtk3 pkgs.wayland pkgs.libxkbcommon ];
  buildPhase = ''
    wayland-scanner client-header protocols/input-method-unstable-v2.xml input-method-client.h
    wayland-scanner private-code protocols/input-method-unstable-v2.xml input-method-protocol.c
    $CC -shared -fPIC -O2 -Wall -Wextra -Wno-unused-parameter \
      input-method.c input-method-protocol.c -o libcharacter-input.so \
      $(pkg-config --cflags --libs gtk+-wayland-3.0 wayland-client xkbcommon)
  '';
  installPhase = ''
    mkdir -p $out/share/character-viewer $out/bin
    cp ${../lumen-desktop/appearance/palette.json} $out/share/character-viewer/palette.json
    cp libcharacter-input.so $out/share/character-viewer/
    cp -r viewer.py replacements.py emoji-test.txt UNICODE-LICENSE.txt icons $out/share/character-viewer/
    makeWrapper ${python}/bin/python3 $out/bin/lumen-character-viewer \
      --add-flags "$out/share/character-viewer/viewer.py" \
      --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.sway pkgs.wtype pkgs.wl-clipboard pkgs.libnotify ]} \
      --set GDK_BACKEND wayland \
      --set CHARACTER_VIEWER_FONTCONFIG ${pkgs.fontconfig.lib}/lib/libfontconfig.so.1 \
      --set CHARACTER_VIEWER_FONT ${font}/share/fonts/noto/NotoColorEmoji.ttf
  '';
}
