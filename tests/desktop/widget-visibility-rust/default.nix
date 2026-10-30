{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-widget-visibility-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    mkdir -p "$out/share/lumen-widget-visibility-fixture"
    cp fixture.qml "$out/share/lumen-widget-visibility-fixture/fixture.qml"
    cp ${../fixtures/settings_fixture.nu} "$out/share/lumen-widget-visibility-fixture/settings_fixture.nu"
    cp ${../fixtures/settings_adapter.nu} "$out/share/lumen-widget-visibility-fixture/settings_adapter.nu"
    cp ${../fixture_calendar_events.nu} "$out/share/lumen-widget-visibility-fixture/fixture_calendar_events.nu"
  '';
}
