# Reduced motion lifecycle acceptance

`tests/desktop/desktop-policy-rust/src/reduced_motion.rs` compares normal and reduced motion using production Help, Dock, session confirmation, HUD and notification components. Its QML scene is `tests/desktop/desktop-policy-rust/reduced_motion.qml`. Each run uses a private application namespace and the production Rust notification service on a private D-Bus; no session actions execute.

The fixture records bounded actual Dock width and banner opacity trajectories, requires intermediate values only in normal mode, and compares settled native geometry, workspace, query, action and provider state. Physical input covers Dock navigation/cancel/activation, Help search/close/reopen, workspace switching, all four session actions' default Cancel, deliberately focused Sleep confirmation, notification dismissal and input after shell restart. Minimize/restore uses an exact captured native container. Actual HUD and notification pixels are required.

Session confirmations keep Cancel as the default. Sleep retains its prominent appearance, but cannot register a competing window-wide Return shortcut; deliberately focused Sleep still responds to Return exactly once.

CI runs both compositor variants through `tests/ci/integration-native.nu` and `tests/ci/run-native.nu`, inside the Rust application isolation runner, and retains `artifacts/reduced-motion-*`, including trajectories, captures and failure state. This is engineering lifecycle coverage; it does not establish reference timings, every navigation route, menu fading or screensaver motion.
