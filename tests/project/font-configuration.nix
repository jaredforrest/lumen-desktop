{ pkgs }:
let
  configuration = (import ../../packages/font-configuration.nix { inherit pkgs; }) {
    fontDirectories = [ "${pkgs.dejavu_fonts}/share/fonts" ];
  };
in builtins.derivation {
  name = "lumen-font-configuration-validation";
  system = pkgs.stdenv.hostPlatform.system;
  builder = "${pkgs.nushell}/bin/nu";
  args = [ "${./font_configuration.nu}" "${configuration}" "${configuration.xmlCatalog}" ];
  PATH = pkgs.lib.makeBinPath [ pkgs.libxml2 pkgs.coreutils ];
}
