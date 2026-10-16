{ pkgs }:
let python = pkgs.python3.withPackages (p: [ p.dbus-python p.pygobject3 ]);
in pkgs.stdenvNoCC.mkDerivation {
  pname = "lumen-global-menu-runtime";
  version = "1.0";
  src = ../res/lumen-desktop/global-menu;
  nativeBuildInputs = [ pkgs.makeWrapper pkgs.wrapGAppsHook3 pkgs.gobject-introspection ];
  buildInputs = [ pkgs.gtk3 ];
  dontBuild = true;
  installPhase = ''
    mkdir -p $out/share/global-menu $out/bin
    cp bridge.py export-helper.py menu-adapter.cjs $out/share/global-menu/
    makeWrapper ${python}/bin/python3 $out/bin/global-menu-python \
      --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.sway pkgs.coreutils ]}
    ln -s ../../bin/global-menu-python $out/share/global-menu/python
  '';
}
