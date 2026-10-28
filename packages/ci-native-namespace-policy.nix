{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-native-namespace-policy";
  version = "0.1.0";
  src = ../res/ci/native-namespace-policy;
  cargoLock.lockFile = ../res/ci/native-namespace-policy/Cargo.lock;
  LUMEN_TIMEOUT_BIN = "${pkgs.coreutils}/bin/timeout";
}
