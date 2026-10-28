{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-input-pane-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  LUMEN_NU = "${pkgs.nushell}/bin/nu";
  LUMEN_SETTINGS_ADAPTER = "${../fixtures/settings_adapter.nu}";
}
