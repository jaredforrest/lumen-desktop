{ pkgs }:
# Only for an isolated compositor. Allows Qt to bind the new pointer before clicks.
pkgs.wlrctl.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    sed -i '1i#include <unistd.h>' pointer.c
    sed -i '1i#define _DEFAULT_SOURCE' pointer.c
    ${pkgs.python3}/bin/python3 - <<'PY'
from pathlib import Path
p=Path('pointer.c');s=p.read_text()
needle='\tswitch (cmd->action) {\n\tcase POINTER_ACTION_CLICK:\n\t\tpointer_press'
assert needle in s
s=s.replace(needle,'\twl_display_roundtrip(state->display); usleep(250000);\n'+needle,1)
needle='\t\tpointer_release(cmd->device, cmd->button);'
assert needle in s
s=s.replace(needle,'\t\twl_display_roundtrip(state->display); usleep(100000);\n'+needle,1)
p.write_text(s)
PY
  '';
})
