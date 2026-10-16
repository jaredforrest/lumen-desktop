{ pkgs, profileBridge }:
let
  profileBridge = profileBridge;
  makeCraft = pname: hash: pkgs.stdenvNoCC.mkDerivation {
    inherit pname;
    version = "0.5.0";
    src = pkgs.fetchurl {
      url = "https://github.com/storytold/${pname}/releases/download/v0.5.0/${pname}-0.5.0-linux-x86_64.tar.gz";
      inherit hash;
    };
    nativeBuildInputs = [ pkgs.autoPatchelfHook pkgs.makeWrapper ];
    buildInputs = [ pkgs.stdenv.cc.cc.lib pkgs.alsa-lib ];
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -r bin share $out/
      runHook postInstall
    '';
    postFixup = ''
      mv $out/bin/${pname} $out/bin/.${pname}-native
      makeWrapper ${profileBridge}/bin/lumen-native-profile $out/bin/${pname} \
        --add-flags "${pname} $out/bin/.${pname}-native" \
        --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath [
          pkgs.libxkbcommon pkgs.wayland pkgs.libGL pkgs.vulkan-loader
          pkgs.libx11 pkgs.libxcursor pkgs.libxi pkgs.libxrandr
        ]}
      substituteInPlace $out/share/applications/ai.storyteller.${pname}.desktop \
        --replace-fail "Exec=${pname} " "Exec=$out/bin/${pname} " \
        --replace-fail "TryExec=${pname}" "TryExec=$out/bin/${pname}"
    '';
    meta = {
      homepage = "https://github.com/storytold/${pname}";
      license = with pkgs.lib.licenses; [ mit asl20 ];
      platforms = [ "x86_64-linux" ];
      mainProgram = pname;
    };
  };
in {
  wordcraft = makeCraft "wordcraft" "sha256-mUuzdkee9tdnNc9AKeXvPC1SqFpMms3is3wzsZxXAgM=";
  gridcraft = makeCraft "gridcraft" "sha256-5lBcL5KfmFTDUKDfJhicTlbIrfh8xhYl+A1qjuwrYo0=";
  deckcraft = makeCraft "deckcraft" "sha256-wmSMYHH2MezGY5zmt9YPaO7YPd6rprFVmwfFBGhjEXw=";
}
