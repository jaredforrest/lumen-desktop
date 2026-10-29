{ pkgs }:
# This terminal is a separate user service, outside the failed shell's group.
pkgs.writeShellApplication {
  name = "lumen-session-recovery";
  runtimeInputs = [ pkgs.systemd ];
  text = ''
    result=$(systemctl --user show --property=Result --value lumen-desktop-shell.service)
    if [ "$result" != start-limit-hit ]; then exit 0; fi
    exec ${pkgs.foot}/bin/foot --app-id=lumen-session-recovery --title="Desktop Recovery" \
      ${pkgs.bashInteractive}/bin/bash --noprofile --norc -c '
        printf "%s\\n" \
          "The desktop shell stopped after repeated failures." \
          "Your applications remain open. This terminal is independent of the shell." \
          "Inspect its log with: journalctl --user -u lumen-desktop-shell.service" \
          "When ready, retry with:" \
          "  systemctl --user reset-failed lumen-desktop-shell.service" \
          "  systemctl --user start lumen-desktop-shell.service"
        exec ${pkgs.bashInteractive}/bin/bash --noprofile --norc
      '
  '';
}
