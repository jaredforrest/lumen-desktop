The optional manual **Material hierarchy** workflow renders actual owned Files
content, sidebar and toolbar beside production popover, menu, notification,
brightness tooltip and volume HUD components on a repeating patterned background.
It runs light and dark on the basic and effects compositors inside disposable
filesystem, user, process, network, IPC and UTS namespaces. It opens no real
accounts, audio, display-control devices or personal folders.

Build `.#shell`, `.#compositor-basic`, `.#compositor`, `.#test-application-font`
and `.#test-application-software-driver`, then invoke
the Rust 2024 `lumen-material-hierarchy-fixture` through `tests/ci/run-native.nu`
within `nix develop`. Supply the immutable `--launcher`, `--sway`,
`--font-directory`, `--software-driver` and a fresh `--output`; add `--effects`
for the effects compositor. The manual workflow contains the complete command.

Acceptance checks painted text, semantic foreground contrast, the actual alpha
composition over the captured underlay, distinct opaque document and navigation
roles, and unchanged document samples outside elevated layer bounds. Crops and
last mismatches are retained. Bounds come from production items and native output
geometry. Tooltip visibility is fixture-triggered; this does not test hover.
The private brightness copy exposes its tooltip and disables unrelated hardware
polling; the Files copy disables volume discovery. Exact before/after hashes and
seams are retained. No production styling changes are made.

This complements the existing seven-surface live transparency/input corpus. It
claims engineering material invariants, not reference-exact blur, colours,
shadows, geometry or all application interiors.
