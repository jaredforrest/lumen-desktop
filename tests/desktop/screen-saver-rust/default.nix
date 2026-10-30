{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-screen-saver-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-screen-saver-fixture"
    cp fixture.qml "$out/share/lumen-screen-saver-fixture/fixture.qml"
    cp ${../fixtures/settings_fixture.nu} "$out/share/lumen-screen-saver-fixture/settings_fixture.nu"
    cp ${../fixtures/settings_adapter.nu} "$out/share/lumen-screen-saver-fixture/settings_adapter.nu"
  '';
}
