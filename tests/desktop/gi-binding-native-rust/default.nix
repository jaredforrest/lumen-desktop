{ pkgs }:
let
  pythonBase = import ../../../packages/python-runtime.nix { inherit pkgs; };
  python = pythonBase.withPackages (packages: [ packages.pygobject3 ]);
in (import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-gi-binding-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pythonBase ];
  PYO3_PYTHON = "${pythonBase}/bin/python3";
  LUMEN_GI_PYTHONPATH = "${python}/${pythonBase.sitePackages}";
  LUMEN_GI_TYPELIB_PATH = pkgs.lib.makeSearchPath "lib/girepository-1.0" (map pkgs.lib.getLib [
    pkgs.gobject-introspection pkgs.pango pkgs.gdk-pixbuf pkgs.atk pkgs.harfbuzz
    pkgs.glib pkgs.gtk3 pkgs.at-spi2-core
  ]);
}
