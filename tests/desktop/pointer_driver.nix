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
   int notches;
   unsigned int button_code, button_state;
   while (scanf(" %c", &operation) == 1) {
     if (operation == 'm' && scanf("%lf %lf", &dx, &dy) == 2) pointer_move(cmd->device, wl_fixed_from_double(dx), wl_fixed_from_double(dy));
     else if (operation == 'w' && scanf("%d", &notches) == 1) {
       zwlr_virtual_pointer_v1_axis_source(cmd->device, WL_POINTER_AXIS_SOURCE_WHEEL);
       zwlr_virtual_pointer_v1_axis_discrete(cmd->device, timestamp(), WL_POINTER_AXIS_VERTICAL_SCROLL,
         wl_fixed_from_double(15.0 * notches), notches);
       zwlr_virtual_pointer_v1_frame(cmd->device);
     }
     else if (operation == 'b' && scanf("%u %u", &button_code, &button_state) == 2) {
       if ((button_code != BTN_LEFT && button_code != BTN_RIGHT && button_code != BTN_MIDDLE) || button_state > 1) {
         fprintf(stderr, "Invalid virtual pointer button command\\n");
         exit(EXIT_FAILURE);
       }
       if (button_state) pointer_press(cmd->device, button_code);
       else pointer_release(cmd->device, button_code);
     }
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
