{ pkgs
, systemctl ? "${pkgs.systemd}/bin/systemctl"
, terminal ? "${pkgs.foot}/bin/foot"
, shell ? "${pkgs.bashInteractive}/bin/bash"
}:
pkgs.runCommand "lumen-session-recovery" {
  nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
  LUMEN_RECOVERY_SYSTEMCTL = systemctl;
  LUMEN_RECOVERY_TERMINAL = terminal;
  LUMEN_RECOVERY_SHELL = shell;
} ''
  mkdir -p "$out/bin"
  rustc --edition=2024 -D warnings ${./session-recovery.rs} -o "$out/bin/lumen-session-recovery"
''
