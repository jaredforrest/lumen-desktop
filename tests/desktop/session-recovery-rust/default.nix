{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-session-recovery-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  postInstall = ''
    for endpoint in systemctl foot recovery-shell; do
      ln -s lumen-session-recovery-fixture "$out/bin/$endpoint"
    done
  '';
}
