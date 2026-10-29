{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-lifecycle-native-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
}
