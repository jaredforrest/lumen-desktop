{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-live-appearance-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-live-appearance-fixture"
    cp fixture.qml "$out/share/lumen-live-appearance-fixture/fixture.qml"
  '';
}
