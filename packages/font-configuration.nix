{ pkgs }:
arguments:
let
  catalog = pkgs.writeText "lumen-font-xml-catalog" ''
    <?xml version="1.0"?>
    <catalog xmlns="urn:oasis:names:tc:entity:xmlns:xml:catalog">
      <system systemId="urn:fontconfig:fonts.dtd" uri="${pkgs.fontconfig.out}/share/xml/fontconfig/fonts.dtd"/>
    </catalog>
  '';
in (pkgs.makeFontsConf arguments).overrideAttrs (old: {
  buildCommand = ''
    # Setup hooks reset catalogue variables before this command runs.
    export XML_CATALOG_FILES=${catalog}
    if ! ( ${old.buildCommand} ) 2>font-diagnostics; then
      cat font-diagnostics >&2
      exit 1
    fi
    if test -s font-diagnostics; then
      cat font-diagnostics >&2
      exit 1
    fi
  '';
  passthru.xmlCatalog = catalog;
})
