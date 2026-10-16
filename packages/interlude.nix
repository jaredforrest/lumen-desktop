{ pkgs, lib ? pkgs.lib }:
pkgs.stdenvNoCC.mkDerivation {
    pname = "interlude";
    version = "1.2.3";
    src = pkgs.fetchzip {
      name = "interlude-1.2.3-source";
      url = "https://github.com/avanturation/interlude/archive/refs/tags/1.2.3.tar.gz";
      hash = "sha256-+6VoFeghMds3sLp94CjWAR1kxYIoTGBphzv9f/E2IO8=";
    };
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      install -Dm644 fonts/InterludeVariable.ttf \
        $out/share/fonts/truetype/InterludeVariable.ttf
      runHook postInstall
    '';
    meta = {
      description = "Interface typeface balancing Latin and CJK text";
      homepage = "https://github.com/avanturation/interlude";
      license = lib.licenses.ofl;
      platforms = lib.platforms.all;
    };
  }
