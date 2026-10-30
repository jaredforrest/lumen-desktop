{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-notification-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-notification-native-fixture"
    cp lock.qml recovery.qml focus_presentation.qml "$out/share/lumen-notification-native-fixture/"
  '';
}
