{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "lumen-document-history-safety";
  version = "0.1.0";
  src = pkgs.lib.fileset.toSource {
    root = ../res/office;
    fileset = pkgs.lib.fileset.unions [ ../res/office/document-history ../res/office/atomic_output.rs ];
  };
  sourceRoot = "source/document-history";
  cargoLock.lockFile = ../res/office/document-history/Cargo.lock;
  RUSTFLAGS = "-D warnings";
  installPhase = ''
    mkdir -p "$out"
    touch "$out/passed"
  '';
}
