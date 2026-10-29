{ pkgs }:
let
  vnc = pkgs.libvncserver.overrideAttrs (old: {
    patches = (old.patches or []) ++ [ ../res/lumen-desktop/remote-client/vnc-peer-identity.patch ];
  });
in (pkgs.remmina.override { libvncserver = vnc; }).overrideAttrs (old: {
  patches = (old.patches or []) ++ [ ../res/lumen-desktop/remote-client/rdp-certificate-name.patch ../res/lumen-desktop/remote-client/windowed-toolbar-lifetime.patch ];
  passthru = (old.passthru or {}) // { lumenVnc = vnc; };
  postPatch = (old.postPatch or "") + ''
    substituteInPlace src/remmina_ssh.c \
      --replace-fail 'remmina_file_get_int(remminafile, is_tunnel ? "ssh_tunnel_stricthostkeycheck" : "ssh_stricthostkeycheck", 0)' \
        'remmina_file_get_int(remminafile, is_tunnel ? "ssh_tunnel_stricthostkeycheck" : "ssh_stricthostkeycheck", 1)'
  '';
})
