{ pkgs }:
let
  fileTags = pkgs.callPackage ./file-tags.nix { };
  filePreview = pkgs.callPackage ./file-preview.nix { };
in
(import ./owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-file-surfaces-rust-wip";
  version = "0.1.0";
  src = ../res/lumen-desktop/file-surfaces;
  cargoLock.lockFile = ../res/lumen-desktop/file-surfaces/Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.glib ];
  nativeCheckInputs = [ pkgs.coreutils ];
  LUMEN_TEST_FILE_TAGS = "${fileTags}/bin/lumen-file-tags";
  LUMEN_TEST_FILE_PREVIEW = "${filePreview}/bin/lumen-file-preview";
}
