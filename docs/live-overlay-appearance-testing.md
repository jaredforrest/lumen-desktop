# Live overlay appearance acceptance

`live_overlay_appearance_native.py` complements the existing six utility-window
palette test and seven-family transparency test. It keeps the production
application launcher, application switcher, Window Overview, Control Centre and
Notification Centre open while atomically replacing the watched appearance file.
It exercises light → dark → reduced dark → reduced light → ordinary light in the
same process and mapped surface, rather than comparing separate process starts.

Each transition checks the actual rendered background/material bytes and primary
text ink, absence of the previous palette background, text colors and geometry,
window/body geometry, focus and retained query/selection. Opaque fallback is
required for rendered material rectangles when transparency reduction is active;
the software compositor also requires opaque materials. Physical keyboard events
establish the launcher/Overview queries and switcher selection. After all live
updates, physical Escape closes those surfaces; the focused Control Centre
settings button and notification card dispatch only their captured private
observer actions. No external application, settings or notification action runs.

The runner uses a frozen source boundary and a disposable user/PID/mount/network
namespace, private activation-free D-Bus, private HOME and fixed synthetic
provider state. Production component code is copied unchanged. Only the Overview
status backend and brightness discovery are replaced with hashed, fixed,
read-only synthetic providers. A missing private Bluetooth service is expected
and does not imply missing physical hardware. Effects acceptance explicitly uses
the established software graphics capability; ordinary acceptance exposes no
graphics device. Every owned child is reaped.

These are local palette/material, geometry and keyboard lifecycle checks. They
make no reference screenshot equivalence, hardware/device, global desktop
activation, third-party application, or real consumer configuration persistence
claim. Existing calendar, Dock, notification-card, generic popover and utility
window proofs retain their separate scopes. No production defect was found in
the added five-family corpus.
