{ pkgs, base }:
let
  runtime = import ./global-menu-runtime.nix { inherit pkgs; };
  asarPatch = import ./asar-patch.nix { inherit pkgs; };
in base.overrideAttrs (old: {
  nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ asarPatch pkgs.makeWrapper ];
  postInstall = (old.postInstall or "") + ''
    ${asarPatch}/bin/lumen-asar-patch "$out/opt/codex-desktop/resources/app.asar" --native-frames
    wrapProgram "$out/bin/codex-desktop" \
      --set CODEX_MENU_ADAPTER_ROOT ${runtime}/share/global-menu \
      --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.bash pkgs.coreutils pkgs.diffutils pkgs.findutils ]}
  '';
})
