{ pkgs, lib ? pkgs.lib }:
let
  source = pkgs.fetchzip {
    name = "interlude-1.2.3-source";
    url = "https://github.com/avanturation/interlude/archive/refs/tags/1.2.3.tar.gz";
    hash = "sha256-+6VoFeghMds3sLp94CjWAR1kxYIoTGBphzv9f/E2IO8=";
  };
  package = builtins.derivation {
    name = "interlude-1.2.3";
    system = pkgs.stdenv.hostPlatform.system;
    builder = "${pkgs.nushell}/bin/nu";
    args = [ "${./font-install.nu}" "${source}" ];
    PATH = pkgs.lib.makeBinPath [ pkgs.coreutils ];
  };
in pkgs.lib.extendDerivation true {
  meta = {
    description = "Interface typeface balancing Latin and CJK text";
    homepage = "https://github.com/avanturation/interlude";
    license = lib.licenses.ofl;
    platforms = lib.platforms.all;
  };
} package
