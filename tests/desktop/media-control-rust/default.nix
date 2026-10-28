{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-media-control-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-media-control-fixture"
    cp fixture.qml "$out/share/lumen-media-control-fixture/fixture.qml"
  '';
}
