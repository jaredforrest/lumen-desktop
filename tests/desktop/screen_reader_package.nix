{ pkgs }:
let
  readerAtspi = import ./screen_reader_atspi.nix { inherit pkgs; };
  desktopPython = import ../../packages/python-runtime.nix { inherit pkgs; };
  readerPython = pkgs.python3.override {
    packageOverrides = final: previous: {
      inherit (desktopPython.pkgs) pygobject3 dbus-python;
      at-spi2-core = readerAtspi;
      dasbus = previous.dasbus.overrideAttrs (old: {
        patches = (old.patches or [ ]) ++ [ ./dasbus-closures.patch ];
      });
    };
  };
in (pkgs.orca.override { python3 = readerPython; at-spi2-core = readerAtspi; }).overrideAttrs (old: {
  patches = (old.patches or [ ]) ++ [ ./orca-glib-unix.patch ];
  preFixup = (old.preFixup or "") + ''
    makeWrapperArgs+=(--prefix GI_TYPELIB_PATH : ${readerAtspi}/lib/girepository-1.0)
  '';
})
