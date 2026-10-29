{ pkgs, assets ? ../res/lumen-desktop, enabled ? false }:
let
  reader = pkgs.callPackage ./reader-commands-rust.nix { };
in pkgs.writeTextFile {
  name = "lumen-reader-command";
  destination = "/bin/lumen-reader-command";
  executable = true;
  text = ''
    #!${pkgs.nushell}/bin/nu
    def --wrapped main [...args] {
      if ($args | length) != 1 or ($args.0? | default "") not-in [where-am-i read-all stop] {
        print '{"accepted":false,"reason":"unsupported_command","dispatched":false}'
        exit 1
      }
      exec ${pkgs.coreutils}/bin/timeout --signal=KILL 4s ${reader}/bin/lumen-reader-commands ${pkgs.lib.optionalString enabled "--enabled"} ...$args
    }
  '';
}
