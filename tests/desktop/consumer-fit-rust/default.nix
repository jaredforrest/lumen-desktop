{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-consumer-fit-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-consumer-fit-fixture"
    cp fixture.qml "$out/share/lumen-consumer-fit-fixture/"
  '';
}
