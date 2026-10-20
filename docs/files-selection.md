# Files selection

File focus, the selected set and the range anchor are independent. Refresh and
sorting restore exact identity references and clear a missing focus instead of
moving it to the first selected row. An explicit user selection retires a pending
scroll/presentation restoration, so its deferred callback cannot erase the new
selection during an immediate refresh.

Blank-area band selection uses a shared viewport component and pure selection
model for list, icon and gallery views and the active leaf column. The leaf
column query and ordered identity rows must match the active folder. Ancestor
columns retain navigation. A press inside an existing row remains available to
its original file-drag, click or context-menu implementation. Scrollbar strips
are excluded. A visible 12-pixel blank selection gutter is reserved next to
vertical list/column rows, between icon cells and above the gallery scrollbar,
including fully packed folders. This is a provisional engineering affordance, not
a reference geometry measurement. Row interaction stays within the actual
delegate; hit testing does not guess which text pixels are painted.
The component changes only a painted preview until a valid release.
Plain drag replaces the selected set, Control toggles membership against the
captured set, and Shift adds membership. These are declared engineering policies.
Focus and anchor retain their captured identities rather than following the last
row crossed by the rectangle.

A changed folder, tab, view, output context, row identity/order, viewport geometry,
selection, focus or lock permission invalidates the gesture. Escape restores the
prior selection and content focus. Other focus loss cancels without stealing the
new focus. Wheel input remains available to the Flickable; scrolling cancels the
pending band instead of guessing unobserved offscreen item geometry. This bounded
workflow does not advertise edge autoscroll. All geometry comes from instantiated
view delegates and is captured once; the pointer-move path does not repeatedly
scan an entire directory. Unknown native file identities cannot start a band.
Personal Trash rows publish the same exact decimal identity format as local rows.
Path-changing recovery also requires native observed single-link provenance;
unknown provenance, hard-link ambiguity and directory link counts never imply
that another name is the same selected directory entry. Lost selection/focus has
a visible notice. Ordinary single-link file rename and same-path sorting remain
supported. No complete history of unobserved namespace events is claimed.

`test_files_selection_model.cjs` covers modifier policy, exact references,
independent focus/anchor, geometry and changed-revision refusal.
`test_files_selection.py` reads a disposable personal Trash entry without deleting
anything. `test_files_selection_qml.py` loads actual Files components offscreen and
exercises sort/replacement focus, deferred-restore retirement, production delegate
geometry, preview-only state and cancellation/focus routing. Its direct component
protocol calls do not claim genuine pointer input; native acceptance is a separate
gate.
