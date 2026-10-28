{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-notification-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-notification-native-fixture"
    cp lock.qml recovery.qml "$out/share/lumen-notification-native-fixture/"
  '';
}
