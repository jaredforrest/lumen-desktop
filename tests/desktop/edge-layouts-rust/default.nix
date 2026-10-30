{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-edge-layouts-fixture";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [ ./. ../application-isolation ];
  };
  sourceRoot = "source/edge-layouts-rust";
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.makeWrapper ];
  LUMEN_EDGE_ISOLATION = "${pkgs.callPackage ../application-isolation { }}/bin/lumen-application-isolation";
  LUMEN_EDGE_HELPER_PATH = pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.dbus pkgs.git pkgs.foot pkgs.grim pkgs.wtype ];
  postFixup = ''
    wrapProgram "$out/bin/lumen-edge-layouts-fixture" \
      --set PATH "${pkgs.lib.makeBinPath [ pkgs.coreutils pkgs.dbus pkgs.git pkgs.foot pkgs.grim pkgs.wtype ]}"
  '';
}
