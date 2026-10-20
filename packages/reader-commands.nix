{ pkgs, assets ? ../res/lumen-desktop, enabled ? false }:
let
  python = (import ./python-runtime.nix { inherit pkgs; }).withPackages (p: [ p.dbus-python ]);
in pkgs.writeShellApplication {
  name = "lumen-reader-command";
  runtimeInputs = [ pkgs.coreutils ];
  text = ''
    if [ "$#" -ne 1 ]; then
      printf '%s\n' '{"accepted":false,"reason":"unsupported_command","dispatched":false}'
      exit 1
    fi
    case "$1" in
      where-am-i|read-all|stop) ;;
      *) printf '%s\n' '{"accepted":false,"reason":"unsupported_command","dispatched":false}'; exit 1 ;;
    esac
    exec timeout --signal=KILL 4s ${python}/bin/python3 ${assets}/runtime/reader_commands.py ${pkgs.lib.optionalString enabled "--enabled"} "$@"
  '';
}
