{ pkgs }:
# Persistent virtual pointer for isolated input tests. Commands arrive on stdin.
pkgs.wlrctl.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    ${pkgs.python3}/bin/python3 - <<'PY'
from pathlib import Path
p=Path('pointer.c');s=p.read_text()
needle='\tswitch (cmd->action) {\n\tcase POINTER_ACTION_CLICK:\n\t\tpointer_press'
replacement="""
 if (cmd->action == POINTER_ACTION_MOTION) {
   char operation;
   double dx, dy;
   while (scanf(" %c", &operation) == 1) {
     if (operation == 'm' && scanf("%lf %lf", &dx, &dy) == 2) pointer_move(cmd->device, wl_fixed_from_double(dx), wl_fixed_from_double(dy));
     else if (operation == 'p') pointer_press(cmd->device, BTN_LEFT);
     else if (operation == 'r') pointer_release(cmd->device, BTN_LEFT);
     wl_display_roundtrip(state->display);
   }
 } else switch (cmd->action) {
 case POINTER_ACTION_CLICK:
   pointer_press"""
assert needle in s
p.write_text(s.replace(needle,replacement,1))
PY
  '';
})
