# Force Quit acceptance boundary

The Rust 2024 `lumen-force-quit-native --acceptance` fixture uses the production confirmation component
and installed application-control broker without replacing their state machines
or process backend. Its runner creates a private compositor, HOME, runtime and
session bus. Only disposable native Rust GTK
clients are stopped or terminated.

The fixture proves keyboard Escape cancellation, focused Tab/Space confirmation
of an exact stopped native process, rejection of a retired window with a same-title
replacement, and mixed-application refusal. A second client retains its synthetic
dirty text and receives physical keyboard edits after each operation. The stale
confirmation error must return focus to Close and remain dismissible by Escape.
It checks the compositor window identity without persisting the one-use
confirmation capability.

This is not automatic hung-application detection or an application save-dialog
test. A synthetic dirty flag does not represent another provider's save protocol.
The broker still rejects shared unselected windows and changed process identities. Return is not treated as implicit destructive
confirmation; Space activates the explicitly focused button. Each confirmation
is consumed before revalidation/signalling and never retried against a new target.

Both native renderer variants run this fixture in CI.

XWayland selections resolve the window's local connection PID through the XRes
extension and require it to match the compositor PID at both prepare and
confirmation. The untrusted `_NET_WM_PID` property alone never authorizes a
signal. The same process start time, executable identity, pidfd and one-use
confirmation checks apply to native and verified XWayland applications.
A missing XRes identity fails with an explanation rather than guessing a PID.
