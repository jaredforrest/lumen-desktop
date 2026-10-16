{ pkgs }:
# Test-only virtual pointer: press, move over a folder, wait, release without
# another motion event. Run only against the isolated compositor in smoke_installed.
pkgs.wlrctl.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    sed -i '1i#include <unistd.h>' pointer.c
    sed -i '1i#define _DEFAULT_SOURCE' pointer.c
    ${pkgs.python3}/bin/python3 - <<'PY'
from pathlib import Path
p=Path('pointer.c');s=p.read_text()
old='\tswitch (cmd->action) {\n\tcase POINTER_ACTION_CLICK:\n\t\tpointer_press'
new="""if (cmd->action == POINTER_ACTION_MOTION) {
 wl_display_roundtrip(state->display); usleep(200000);
 pointer_press(cmd->device, BTN_LEFT); wl_display_roundtrip(state->display); usleep(200000);
 for(int i=0;i<14;i++){pointer_move(cmd->device,cmd->dx/14,cmd->dy/14);wl_display_roundtrip(state->display);usleep(50000);}
 usleep(1500000); pointer_release(cmd->device,BTN_LEFT);wl_display_roundtrip(state->display);
} else switch (cmd->action) {
 case POINTER_ACTION_CLICK:
  pointer_press"""
assert old in s
p.write_text(s.replace(old,new))
PY
  '';
})
