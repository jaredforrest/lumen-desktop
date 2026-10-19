{ pkgs }:
let
  agent = pkgs.polkit_gnome.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../res/auth/polkit-password-visibility.patch ];
  });
  schemaDir = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}/glib-2.0/schemas";
  bridge = pkgs.runCommand "gtk-session-appearance" {
    nativeBuildInputs = [ pkgs.pkg-config pkgs.stdenv.cc ];
    buildInputs = [ pkgs.gtk3 ];
  } ''
    mkdir -p $out/lib/gtk-3.0/modules
    $CC -Wall -Wextra -Werror -shared -fPIC \
      -DDESKTOP_SCHEMA_DIR='"${schemaDir}"' \
      $(pkg-config --cflags gtk+-3.0 gmodule-2.0) \
      ${../res/auth/gtk-session-appearance.c} \
      $(pkg-config --libs gtk+-3.0 gmodule-2.0) \
      -o $out/lib/gtk-3.0/modules/libsession-appearance.so
  '';
in pkgs.symlinkJoin {
  name = "polkit-gnome-session-appearance";
  paths = [ agent ];
  nativeBuildInputs = [ pkgs.makeWrapper ];
  postBuild = ''
    wrapProgram $out/libexec/polkit-gnome-authentication-agent-1 \
      --prefix GTK_MODULES : ${bridge}/lib/gtk-3.0/modules/libsession-appearance.so
  '';
  passthru = { inherit bridge; };
}
