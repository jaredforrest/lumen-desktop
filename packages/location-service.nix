{ pkgs }:
let runtime = pkgs.callPackage ./appearance-rust.nix {};
in pkgs.runCommand "lumen-location-service" {} ''
  mkdir -p $out/lib/systemd/system $out/share/lumen
  ln -s ${runtime} $out/share/lumen/runtime
  cat > $out/lib/systemd/system/lumen-location.service <<'UNIT'
  [Unit]
  Description=Refresh local timezone and desktop solar location
  After=network-online.target
  Wants=network-online.target
  [Service]
  Type=oneshot
  ExecStart=${runtime}/bin/lumen-location
  Environment=PATH=${pkgs.lib.makeBinPath [pkgs.curl pkgs.iproute2]}
  Environment=SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
  StateDirectory=lumen-location
  StateDirectoryMode=0755
  UMask=0022
  TimeoutStartSec=30
  MemoryMax=128M
  NoNewPrivileges=true
  ProtectHome=true
  ProtectSystem=strict
  ReadWritePaths=/etc /var/lib/lumen-location
  PrivateTmp=true
  UNIT
  cat > $out/lib/systemd/system/lumen-location.timer <<'UNIT'
  [Unit]
  Description=Keep local timezone and solar appearance current
  [Timer]
  OnBootSec=30s
  OnCalendar=*:0/15
  Persistent=true
  RandomizedDelaySec=15s
  [Install]
  WantedBy=timers.target
  UNIT
''
