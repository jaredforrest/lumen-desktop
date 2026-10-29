{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-application-isolation";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.git ];
  LUMEN_BWRAP = "${pkgs.bubblewrap}/bin/bwrap";
}
