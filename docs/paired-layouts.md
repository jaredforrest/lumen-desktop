# Paired full-output placement

Two compatible application windows can share a dedicated desktop. The native
compositor owns the pair, its window identities, divider ratio and each member's
previous placement. The shell provides a partner chooser and management overlay;
that overlay stays above the full-output pair without becoming a third pane.

Open the green-button layout menu and choose **Pair Full Screen on Left** or
**Pair Full Screen on Right**, then select the other window. **Manage Paired
Windows** offers replacement, side swapping, division reset and return to desktop.
Drag the shared divider to change the ratio. Double-click it to reset the division;
Escape cancels an unfinished divider drag. Client size constraints clamp ratios,
and incompatible clients are rejected before placement changes.

Returning either member to its desktop restores its saved reachable placement.
The other member remains full screen in the dedicated desktop, with its own saved
placement retained for a later exit. Closing or externally moving a member follows
the same survivor lifecycle. Replacing a member restores the outgoing window and
captures the incoming window's own placement. An initially full-screen member
returns to its saved ordinary geometry when it exits pairing.

Show Desktop temporarily hides and restores a pair as one group. Activating one
hidden member restores the group. Application hiding and ordinary layout actions
first exit pairing, preserving their normal restore semantics. Minimization is
disabled while a window belongs to the pair or remains its full-screen survivor.
New unrelated windows map to an ordinary desktop; they cannot become a third
paired child. Transient dialogs follow the compositor's smart-popup policy and
remain reachable above their paired application. They follow that exact owner on
exit, replacement or external movement; surviving dialogs return to an ordinary
desktop when their owner closes. Protocol-owned and modal dialogs cannot be
primary paired members. Relationship changes are reconciled during native
arrangement: detached dialogs become independent ordinary windows, and a changed
actual owner replaces the captured owner. An ordinary window with already-open untracked dialogs
is rejected rather than moving only part of that window group. Existing no-focus rules continue
to apply to unrelated new windows.

## Native contract

`get_version` advertises `native_paired_layouts`. Each native window's tree record
contains a versioned `lumen_pair` object: `eligible`, `survivor`, and, for a paired
member, `id`, `partner`, `side` and `ratio`. IDs are compositor container IDs.
Commands use exact IDs and never substitute the focused window:

```
lumen_pair create LEFT_ID RIGHT_ID
lumen_pair replace KEPT_ID NEW_PARTNER_ID
lumen_pair swap MEMBER_ID
lumen_pair ratio MEMBER_ID FRACTION
lumen_pair leave MEMBER_OR_SURVIVOR_ID
```

`FRACTION` must be finite and strictly between zero and one. The effective value
is clamped to both clients' limits. Invalid, closed, fixed-size or incompatible
identities fail before changes. Shell requests additionally validate the captured
process ID before sending the native command. A closed selected partner clears
selection and reports an error; another window is never selected as a fallback.

The implementation lives in the `lumen_pair.c`/`.h` native module carried by
`sway-paired-layouts.patch`, with narrow lifecycle hooks. `window_pair.py` is the
shell adapter and `WindowPairPicker.qml` owns the overlay. Machine configuration
contains no pairing algorithms or window restore state.

## Verification and limits

`paired_layouts_input.py` runs real pointer gestures in disposable compositors.
It covers divider drag/cancellation/reset, group reveal, application hiding,
replacement, swap, both exit paths, partner destruction during a drag, external
removal, fixed and bounded size constraints, new application mapping, a real
resizable protocol transient through owner exit/replacement/movement/destruction,
explicit parent removal, and output loss with reachable restoration. Both compositor
variants are tested. `paired_picker_qml.py` loads the real menu, chooser and
adapter, including a selected partner disappearing before confirmation.

Divider hit width, double-click interval and visual treatment are implementation
choices pending reference measurement. These tests establish functional lifecycle
and identity behavior, not exact reference appearance or timing. The compositor
binary is selected at login; installing it does not replace a running session.
