# Clock activation acceptance

`lumen-clock-native-fixture` (Rust 2024) exercises the installed full shell in a disposable private compositor, home, network namespace and activation-free session bus. It verifies the installed clock, notification panel, calendar and shell source bytes before adding reversible read-only geometry/status observers. Unrelated providers are fixed unavailable; a fixed local calendar provider and the production private notification service supply synthetic content.

The fixture sends actual pointer press/release events to the production clock, observes the populated Notification Centre, presses the clock again, then reopens and dismisses by an outside click and Escape. It checks measured viewport bounds, retained notification identity and exact pre-popup native window focus. There is no control-method activation substitute, account connection or host input. Source hashes, package hashes, screenshots, compositor logs and final state are retained in CI for both compositor variants.

This establishes the semantic open/toggle/dismiss path. It does not establish reference pixel measurements, audible accessibility, account synchronization or application content interoperability.
