{ pkgs }:
(import ../../../packages/owned-rust.nix { inherit pkgs; }) {
  pname = "lumen-desktop-surface-fixture";
  version = "0.1.0";
  src = ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.gtk3 ];
  LUMEN_FIXTURE_SOFTWARE_DRIVER = "${pkgs.mesa}";
  LUMEN_FIXTURE_FONTCONFIG = (import ../../../packages/font-configuration.nix { inherit pkgs; }) {
    fontDirectories = [ "${pkgs.callPackage ../../../packages/interlude.nix {}}/share/fonts" "${pkgs.dejavu_fonts}/share/fonts" ];
  };
}
