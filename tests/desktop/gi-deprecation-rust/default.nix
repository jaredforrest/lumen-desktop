{ pkgs }:
let
  pythonBase = import ../../../packages/python-runtime.nix { inherit pkgs; };
  python = pythonBase.withPackages (packages: [ packages.pygobject3 ]);
in
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-gi-deprecation-check";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pythonBase ];
  PYO3_PYTHON = "${pythonBase}/bin/python3";
  PYTHONPATH = "${python}/${pythonBase.sitePackages}";
  GI_TYPELIB_PATH = pkgs.lib.makeSearchPath "lib/girepository-1.0" [ pkgs.glib pkgs.gtk3 pkgs.libical pkgs.evolution-data-server pkgs.gnome-online-accounts ];
  # GLib's event loop must run on Python's process main thread, outside Cargo's test workers.
  postInstall = ''
    "$out/bin/lumen-gi-deprecation-check"
  '';
}
