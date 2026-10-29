{ pkgs }:
let client = import ../../../packages/remote-client.nix { inherit pkgs; };
in pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-remote-peer-identity-check";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  buildInputs = [ client.lumenVnc pkgs.openssl ];
}
