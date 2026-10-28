{ pkgs }:
let
  atspi = pkgs.at-spi2-core;
  output = builtins.derivation {
    name = "screen-reader-atspi-${atspi.version}";
    system = pkgs.stdenv.hostPlatform.system;
    builder = "${pkgs.nushell}/bin/nu";
    args = [
      "${./screen_reader_atspi.nu}"
      "${atspi}"
      "${atspi.dev}/share/gir-1.0/Atspi-2.0.gir"
      "${pkgs.glib.dev}/share/gir-1.0"
      "${./correct_atspi_action_aliases.nu}"
    ];
    PATH = pkgs.lib.makeBinPath [ pkgs.nushell pkgs.coreutils pkgs.findutils pkgs.gobject-introspection.dev ];
  };
in output // { inherit (atspi) version dev; }
