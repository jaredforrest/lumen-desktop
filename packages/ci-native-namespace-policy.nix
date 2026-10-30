{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-native-namespace-policy";
  version = "0.1.0";
  src = ../res/ci/native-namespace-policy;
  cargoLock.lockFile = ../res/ci/native-namespace-policy/Cargo.lock;
  LUMEN_TIMEOUT_BIN = "${pkgs.coreutils}/bin/timeout";
  LUMEN_BWRAP_BIN = "${pkgs.bubblewrap}/bin/bwrap";
  LUMEN_UNSHARE_BIN = "${pkgs.util-linux}/bin/unshare";
  LUMEN_ISOLATION_BIN = "${import ../tests/desktop/application-isolation { inherit pkgs; }}/bin/lumen-application-isolation";
}
