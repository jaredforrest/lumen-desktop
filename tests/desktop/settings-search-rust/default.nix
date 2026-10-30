{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-settings-search-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  LUMEN_SETTINGS_SEARCH_DBUS_DAEMON = "${pkgs.dbus}/bin/dbus-daemon";
  LUMEN_SETTINGS_SEARCH_GRIM = "${pkgs.grim}/bin/grim";
}
