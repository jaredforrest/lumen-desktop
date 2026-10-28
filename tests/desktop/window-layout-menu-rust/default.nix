{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-window-layout-menu-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-window-layout-menu-fixture"
    cp fixture.qml "$out/share/lumen-window-layout-menu-fixture/fixture.qml"
  '';
}
