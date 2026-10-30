{ pkgs }:
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-stage-artifacts";
  version = "0.1.0";
  src = ../res/ci/artifact-staging;
  cargoLock.lockFile = ../res/ci/artifact-staging/Cargo.lock;
  postInstall = ''
    mkdir -p "$TMPDIR/artifacts/native-command-logs"
    printf '%s\n' 'Immutable artifact policy validation' > "$TMPDIR/artifacts/native-command-logs/policy-check.txt"
    "$out/bin/lumen-stage-artifacts" "$TMPDIR" "${../tests/ci/backend-artifact-paths.txt}" "$TMPDIR/staged-backend"
    "$out/bin/lumen-stage-artifacts" "$TMPDIR" "${../tests/ci/vault-artifact-paths.txt}" "$TMPDIR/staged-vault"
    "$out/bin/lumen-stage-artifacts" "$TMPDIR" "${../tests/ci/provider-artifact-paths.txt}" "$TMPDIR/staged-provider"
    "$out/bin/lumen-stage-artifacts" "$TMPDIR" "${../tests/ci/material-artifact-paths.txt}" "$TMPDIR/staged-material"
  '';
}
