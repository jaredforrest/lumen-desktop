{ pkgs }:
pkgs.runCommand "screen-reader-atspi-${pkgs.at-spi2-core.version}" {
  nativeBuildInputs = [ pkgs.python3 pkgs.gobject-introspection ];
  passthru = { inherit (pkgs.at-spi2-core) version dev; };
} ''
  mkdir -p "$out"
  cp -rs ${pkgs.at-spi2-core}/. "$out/"
  find "$out" -type d -exec chmod u+w {} +
  rm "$out/lib/girepository-1.0/Atspi-2.0.typelib"
  mkdir -p "$out/share/gir-1.0"
  python3 ${./correct_atspi_action_aliases.py} \
    ${pkgs.at-spi2-core.dev}/share/gir-1.0/Atspi-2.0.gir \
    "$out/share/gir-1.0/Atspi-2.0.gir" "$out/share/action-alias-proof.json"
  g-ir-compiler --includedir=${pkgs.glib.dev}/share/gir-1.0 \
    "$out/share/gir-1.0/Atspi-2.0.gir" \
    --output="$out/lib/girepository-1.0/Atspi-2.0.typelib"
''
