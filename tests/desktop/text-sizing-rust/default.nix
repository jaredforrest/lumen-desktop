{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-text-sizing-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-text-sizing-fixture"
    cp fixture.qml "$out/share/lumen-text-sizing-fixture/fixture.qml"
  '';
}
