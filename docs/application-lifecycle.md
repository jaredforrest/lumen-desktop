# Observed application lifecycle

The shell's lifecycle observer retains an external native application after its
last window closes **while the exact observed process remains alive**. It learns
PID/application associations from the compositor, verifies same-user process
credentials, captures process start time and executable identity, opens a pidfd,
and checks that identity again after opening the handle. Exit readiness or any
identity change removes the observation. A reused PID cannot inherit it.

Running entries feed the Dock indicator and application switcher independently of
window count. Application-level MRU order survives closing the last window during
the observation. A zero-window switcher selection captures the process instance
identities; it cannot activate a different process that later advertises the same
application ID. Activation uses the installed application's normal desktop entry,
which remains responsible for reopening or activating its own instance.

Application IDs remain **advertisements**, not authenticated package identities.
If one process advertises multiple application IDs, process survival does not
establish which individual application is still running. Such associations are
not retained as zero-window applications. Independent processes sharing one app ID
are tracked separately but are not silently merged into one Quit target. The
observer bounds process handles and per-process advertised IDs. X11 clients,
privileged processes, shell-owned windows and unavailable process handles retain
window-based behaviour rather than fabricated process assurance.

## Cooperative Quit

The Dock's **Request Quit** asks for the application's supported command. The
current provider is an enabled, parameterless `quit` action on `org.gtk.Actions`.
The bus name must equal the native advertised application ID, its unique owner
must have the exact observed PID, and process identity plus name ownership are
revalidated before dispatch. No bus-name activation is used for discovery. The
request is reported as requested, not as confirmed process exit.

Applications own dirty-document save/discard/cancel decisions. An unavailable,
disabled, parameterised, ambiguous or changed provider produces an explanation to
use the application's own Quit command. It never becomes Close All, SIGTERM or
SIGKILL. Explicit Force Quit remains the separate confirmed process-termination
workflow; its security checks are unchanged.

## Remaining boundaries

This is an observer, not a complete application broker. It cannot discover a
process that has never exposed an observed native window. Already-observed
identities can be recovered across observer and shell restarts
in the same compositor session. The private runtime journal binds records to the
boot ID and owned compositor socket epoch. Each recovered process must again match
same-user credentials, start time and executable identity before and after a fresh
pidfd is opened. Corrupt, oversized, nonprivate, linked or cross-session records
confer no running state. Atomic replacement and a per-session observer lease avoid
partial records and concurrent writers. This is a saved observation, not an
authentication claim about the advertised application ID. Never-observed zero-
window processes still require new native evidence. Application MRU persistence
across a whole shell restart is separate and is not implemented here. Applications
with different native and bus IDs,
unsupported Quit protocols, or helper-process ownership need explicit adapters.
No command is guessed from a translated menu label. Package identity, active
application/main-window ownership, state restoration and retained-app document
fidelity remain separate contracts.

The disposable native GTK test exercises two in-memory dirty documents with app-
owned cancel, save and discard handlers. It also restarts the observer and then
the whole isolated shell while all app
windows are closed, checking fresh process validation and retained Dock/switcher
activation. It proves routing and non-forced termination, not the document fidelity
of every retained application.
