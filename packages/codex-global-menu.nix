{ pkgs, base }:
let runtime = import ./global-menu-runtime.nix { inherit pkgs; };
in base.overrideAttrs (old: {
  nativeBuildInputs = (old.nativeBuildInputs or []) ++ [ pkgs.python3 pkgs.makeWrapper ];
  postInstall = (old.postInstall or "") + ''
    ${pkgs.python3}/bin/python3 ${../res/lumen-desktop/global-menu/patch-asar.py} \
      "$out/opt/codex-desktop/resources/app.asar"
    wrapProgram "$out/bin/codex-desktop" \
      --set CODEX_MENU_ADAPTER_ROOT ${runtime}/share/global-menu \
      --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.bash pkgs.coreutils pkgs.diffutils pkgs.findutils ]}
  '';
})
