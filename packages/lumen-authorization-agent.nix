{ pkgs }:
let
  agent = pkgs.polkit_gnome.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ../res/auth/polkit-password-visibility.patch ];
  });
  schemaDir = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}/glib-2.0/schemas";
  bridge = pkgs.runCommand "gtk-session-appearance" {
    nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
    DESKTOP_SCHEMA_DIR = schemaDir;
  } ''
    mkdir -p $out/lib/gtk-3.0/modules
    rustc --edition 2024 --crate-type cdylib -D warnings \
      -L native=${pkgs.gtk3}/lib -L native=${pkgs.glib.out}/lib \
      ${../res/auth/gtk-session-appearance.rs} \
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
