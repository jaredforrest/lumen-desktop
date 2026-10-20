# Force Quit acceptance boundary

`tests/desktop/force_quit_native.py` uses the installed confirmation component
and installed application-control broker without replacing their state machines
or process backend. Its outer runner freezes source/tools and creates a private
filesystem, PID, network and session-bus namespace. Only disposable native GTK
clients are stopped or terminated.

The fixture proves keyboard Escape cancellation, focused Tab/Space confirmation
of an exact stopped native process, rejection of a retired window with a same-title
replacement, and mixed-application refusal. A second client retains its synthetic
dirty text and receives physical keyboard edits after each operation. The stale
confirmation error must return focus to Close and remain dismissible by Escape.
It records captured process start/executable/native-container identities and
screenshots without persisting the one-use confirmation capability.

This is not automatic hung-application detection or an application save-dialog
test. A synthetic dirty flag does not represent another provider's save protocol.
The existing broker's conservative native-client policy and rejection of shared
unselected windows remain unchanged. Return is not treated as implicit destructive
confirmation; Space activates the explicitly focused button. Each confirmation
is consumed before revalidation/signalling and never retried against a new target.

Both native renderer variants run this fixture in CI. Logs are scanned after all
owned processes have been reaped; warnings are acceptance failures.
