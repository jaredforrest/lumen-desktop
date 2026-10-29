# Supplementary tooltip lifecycle

Owned battery status, display brightness and constrained Files preview help use
one bounded tooltip lifecycle. Hover waits 700 ms; a tooltip expires after four
seconds and requires a new hover or help request. F1 on a focused control opens
its supplementary help immediately. Escape, pointer exit, pressing, disabling,
owner destruction and session popup replacement retire it. Accessible names and
essential descriptions remain independent of whether the tooltip is visible.

The Dock keeps its nonfocusable layer surface and its own keyboard navigation.
It shares the lifecycle, coordinator, metadata text role, wrapping and bounded
surface styling. Changing the hovered identity restarts the delay; drag/context
states and another popup suppress it. F1 in Dock keyboard mode requests help.
The surface is clamped to the output. This does not add keyboard focus to the
layer or claim exact reference-system appearance.

`lumen-tooltip-fixture` exercises actual synthetic Qt pointer/key input, painted
frames and owner destruction in the existing disposable software boundary, in
both palettes and at default/enlarged text. An optional immutable committed
battery baseline reproduces its missing default hover route and unbounded attached-tooltip policy.
The production Dock motion scenario retains its pointer sweeps and drag checks.
No hardware services, private accounts or real user files are needed.

The longer hover exercise exposed a pre-existing Dock capture race: a periodic
read was treated as a settings edit, and decoded equal preferences replaced the
captured model's identity. Only applying/queued edits now block configuration
interactions. Decoded no-op values preserve identity in both SettingsState and
the watched Theme file; object-key ordering does not count as a change. Genuine
changed values still replace snapshots and the existing strict capture guards
remain in place. The actual settings protocol test covers a queued edit during
a read, and native drags explicitly complete a real read while the pointer is
held before proceeding to their captured target.
