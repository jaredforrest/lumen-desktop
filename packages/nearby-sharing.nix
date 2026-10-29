{ pkgs }:
let providerFixture = pkgs.callPackage ./nearby-provider-fixture.nix { };
in pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-nearby-sharing";
  version = "0.1.0";
  src = ../res/lumen-desktop/nearby-sharing;
  cargoLock.lockFile = ../res/lumen-desktop/nearby-sharing/Cargo.lock;
  LUMEN_NEARBY_TEST_PROVIDER = "${providerFixture}/bin/lumen-nearby-provider-fixture";
}
