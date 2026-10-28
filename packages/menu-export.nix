{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-menu-export";
  version = "0.1.0";
  src = ../res/lumen-desktop/global-menu/export;
  cargoLock.lockFile = ../res/lumen-desktop/global-menu/export/Cargo.lock;
  nativeCheckInputs = [ pkgs.dbus pkgs.coreutils ];
  LUMEN_TEST_DBUS_CONFIG = "${pkgs.dbus}/share/dbus-1/session.conf";
}
