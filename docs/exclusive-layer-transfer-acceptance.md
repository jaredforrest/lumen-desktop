# Exclusive-layer transfer acceptance

The `desktop-transfer-layer` scenario reuses the native group/transfer harness,
production capture and packet API, disposable relationship clients and the real
installed runtime. A private Quickshell `PanelWindow` requests Overlay placement
and Exclusive keyboard focus, following the existing native focus-guard fixture.

The source is first focused, then the layer acquires input. Physical `x` and `y`
keys reach only the layer before and after the root and its nonmodal child move.
Native client key logs remain unchanged during those presses. Exact native
identities, roles, hide states, dimensions and child offsets survive the movement;
the target desktop remains inactive while the layer is open.

After actual layer closure, raw native focus, output-visible desktops and physical
`z` delivery are recorded before assertions. The remaining origin view must receive
the key, and the moved family must not reactivate. Both compositor backends pass
the three-check corpus with the same immutable fixture. CI runs both cases through
the existing private namespace, private terminal and software-renderer boundary;
the existing desktop-transfer artifact allowlist retains their outcomes.

The old provider reproduced the defect: the layer still received both keys during
transfer, but closing it restored the moved source, sent the next key there and
activated the target desktop. The repair updates only an affected retained native
focus history while preserving actual layer keyboard ownership. This corpus does
not infer input ownership from a property alone or use host input devices.
