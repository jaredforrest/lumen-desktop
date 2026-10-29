{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-fixture-providers";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    ln -s lumen-fixture-providers "$out/bin/lumen-fixture-calendar"
  '';
}
