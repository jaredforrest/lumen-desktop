{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-status-clock-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-status-clock-fixture"
    cp fixture.qml "$out/share/lumen-status-clock-fixture/fixture.qml"
  '';
}
