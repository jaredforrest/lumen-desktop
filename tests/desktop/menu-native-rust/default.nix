{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-menu-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-menu-native-fixture"
    cp keyboard.qml.inc menubar.qml.inc "$out/share/lumen-menu-native-fixture/"
  '';
}
