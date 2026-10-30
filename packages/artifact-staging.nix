{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-stage-artifacts";
  version = "0.1.0";
  src = ../res/ci/artifact-staging;
  cargoLock.lockFile = ../res/ci/artifact-staging/Cargo.lock;
  postInstall = ''
    "$out/bin/lumen-stage-artifacts" "$TMPDIR" "${../tests/ci/backend-artifact-paths.txt}" "$TMPDIR/staged-artifacts"
  '';
}
