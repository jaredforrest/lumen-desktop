{ pkgs }:
let
  configuration = (import ../../packages/font-configuration.nix { inherit pkgs; }) {
    fontDirectories = [ "${pkgs.dejavu_fonts}/share/fonts" ];
  };
in pkgs.runCommand "lumen-font-configuration-validation" {
  nativeBuildInputs = [ pkgs.libxml2 ];
} ''
  export XML_CATALOG_FILES=${configuration.xmlCatalog}
  if ! xmllint --valid --noout ${configuration} 2>diagnostics; then
    cat diagnostics >&2
    exit 1
  fi
  if test -s diagnostics; then
    cat diagnostics >&2
    exit 1
  fi
  touch $out
''
