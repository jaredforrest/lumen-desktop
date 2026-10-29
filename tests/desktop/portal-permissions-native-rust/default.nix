{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-portal-permissions-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-portal-permissions-native-fixture"
    cp fixture.qml "$out/share/lumen-portal-permissions-native-fixture/fixture.qml"
  '';
}
