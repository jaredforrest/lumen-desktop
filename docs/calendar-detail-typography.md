# Calendar details text

The owned Calendar details inspector applies the shared text scale to selected dates, event metadata, notes, recurrence/read-only notices and provider status. Body labels and action buttons retain their existing shared roles. At 100% the role sizes are unchanged. Compact Calendar widgets and the event editor already used these roles; the details change does not alter their sizing path.

The inspector's existing focus-aware scroll container keeps lower actions reachable at constrained sizes. Read-only events retain disabled Edit/Delete controls. Notes retain their existing bounded scroll area; the short-note corpus does not claim long-document scrolling coverage.

`calendar_detail_typography_qml.nu` runs the production CalendarWindow with a read-only Rust fixture provider and public resolved font. Its 192 unique frames cover six phases (selected date, details metadata, notes, recurrence/read-only notice, New Event action and provider status), two event permission states, normal/constrained windows, light/dark and 100/115/130/150% text. Each mode/scale/permission segment has a 40-second child deadline; this is a fixture batch bound, not a performance benchmark. Geometry and visible foreground ink are both checked.

The Rust 2024 `calendar-detail-typography` desktop-policy scenario uses a private compositor and keyboard on both supported variants. At constrained 150% light/dark it checks New Event, Cancel returning to the selected details, Edit Occurrence followed by Escape, and Tab between account actions followed by a captured action to an inert shell stub. It checks painted action labels. The private provider allows only reads; no account, external application, calendar mutation or host setting is accessed.

CI builds the public font configuration and runs both commands through `tests/ci/run-native.sh`, retaining captures, provider requests, source/dependency provenance and results. These are engineering layout/input invariants; they do not establish reference-system measurements or third-party application coverage.
