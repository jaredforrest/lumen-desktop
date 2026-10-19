{ pkgs }:
# Isolated compositor only: keep a pointer capability alive while testing hover.
pkgs.wlrctl.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    sed -i '1i#include <unistd.h>' pointer.c
    sed -i '1i#define _DEFAULT_SOURCE' pointer.c
    ${pkgs.python3}/bin/python3 - <<'PY'
from pathlib import Path
p=Path('pointer.c');s=p.read_text()
needle='destroy_pointer(struct wlrctl *state)\n{'
assert needle in s
s=s.replace(needle,needle+'\n\twl_display_roundtrip(state->display); usleep(30000000);',1)
p.write_text(s)
PY
  '';
})
