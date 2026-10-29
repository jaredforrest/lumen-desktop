# Essential dialog and display text

Force Quit, paired-window selection and display configuration consume the shared
text-scale roles. The dialog-heading role retains the previous 17-pixel default;
body, label, metadata and caption roles retain their existing ordinary sizes.

Force Quit measures its body rather than reserving 102 pixels for arbitrary
service errors. Its card grows within the available output and scrolls only when
necessary, revealing the focused action. The pair picker lays out wrapped source,
explanation, candidates, errors and action flows in order, keeping final actions
reachable on short outputs. Display preset buttons wrap into a single column
when three meaningful labels cannot fit, and position fields size to their actual
body font metrics. Captured identities, confirmation phases and provider commands
are unchanged by these presentation repairs.

`essential_dialog_typography_qml.py` presents 48 real production-body cases over
light/dark, 100/115/130/150 percent text, and constrained/ordinary sizes. It checks
role sizes, wrapped label capacity, final-action focus reveal through clipping
ancestors and painted action-label ink. Display controls include a 340-pixel-wide
container. Private status providers reject all mutations. Qt's offscreen platform
cannot supply layer-shell, so the fixture explicitly replaces only the two
layer transport windows/properties with plain Qt windows; the original production
body, focus/action handlers and providers remain otherwise copied byte-for-byte.
This is software presentation proof, not native layer ownership, physical input,
provider mutation or reference-exact geometry acceptance. Owned children exit
before warning scans and final artifact acceptance.

The companion native fixture runs the unmodified layer-shell dialogs on each
compositor in a fully detached private session. At 150 percent on a 700×480
output, physical Return and Escape close the error decision, Space selects an
exact captured pairing candidate, Tab reaches the final pairing action, and
Escape cancels it. Display position typing is observed in the actual field;
physical Tab reveals the final lower action without executing Apply or Forget.
The focused non-destructive Force Quit Close/Cancel control now handles Return
and Enter explicitly; there is no new global destructive default. Synthetic
pair/display status providers refuse all mutations. This finite six-check corpus
per backend complements the software matrix; other errors, pairing states,
provider mutation and actual display hardware remain outside its proof.
