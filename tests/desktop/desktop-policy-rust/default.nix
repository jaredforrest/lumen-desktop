{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-desktop-policy-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-desktop-policy-fixture"
    cp application.qml lifecycle.qml "$out/share/lumen-desktop-policy-fixture/"
  '';
}
