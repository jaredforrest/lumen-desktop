{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-wallpaper-settings-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-wallpaper-settings-fixture"
    cp fixture.qml "$out/share/lumen-wallpaper-settings-fixture/fixture.qml"
    cp ${../fixtures/settings_fixture.nu} "$out/share/lumen-wallpaper-settings-fixture/settings_fixture.nu"
    cp ${../fixtures/settings_adapter.nu} "$out/share/lumen-wallpaper-settings-fixture/settings_adapter.nu"
  '';
}
