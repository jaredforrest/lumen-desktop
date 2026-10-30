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
window processes still require new native evidence. Applications
with different native and bus IDs,
unsupported Quit protocols, or helper-process ownership need explicit adapters.
No command is guessed from a translated menu label. Package identity, active
application/main-window ownership, state restoration and retained-app document
fidelity remain separate contracts.

## Observed application order

The observer records actual native compositor focus as application-level MRU.
Ordered compositor focus receipts preserve brief activations even when a later
tree query already shows another focused window. Up to 4,096 in-memory native
surface witnesses bind those receipts to an exact previously observed process;
a closed surface may supply a queued receipt while that same process survives.
Unknown surfaces and retired process witnesses fail closed. These surface
associations are cleared with the observer or compositor epoch and never saved.
Retired or reassigned surface IDs become inert tombstones within that same
4,096-entry budget, so reusing an ID cannot authenticate a new owner. At the
limit, previously unknown surface receipts remain unsupported until observer
restart; current live shell focus remains available.
Each rank carries exact observed process-instance witnesses. Closing the last
window retains its rank while a witnessed process survives; a new process with
the same advertised application ID cannot inherit that saved rank. Recovery
revalidates process credentials, start time, executable and pidfds before using
order from the same boot and compositor socket epoch.

Schema 3 stores bounded application and native backing-window order beside the
process observations in the same private atomic journal. Schema 1 remains
process-only recovery; schema 2 restores application order without window order.
Unknown, duplicate or
unassociated witnesses and changed scope refuse the saved record. Order is capped
at 512 applications and 2,048 witnesses. The 1 MiB journal budget drops oldest
order entries before sacrificing otherwise valid process recovery.

The shell resolves current desktop-entry identities and merges recovered order
behind focus changes it has already observed. An epoch change clears prior order;
replaced native instances cannot recover an old rank. Unsupported window-only
applications retain the existing in-memory fallback. Order is preference data,
not an activation authorization or authenticated package identity. No QObject,
translated title, pending switcher selection or command is persisted. Preview
and Escape do not promote an application. Missing past focus events while an
observer was unavailable are not reconstructed.

## Native backing-window selection

For supported native surfaces, the observer retains up to 1,024 ordered window
records containing the compositor container ID, opaque foreign-toplevel surface
identifier, advertised application ID and exact process-instance witness. The
same boot/socket scope and 1 MiB journal budget apply. Recovery revalidates all
four fields against live native surfaces and processes; a reused container ID,
even in the same process, cannot inherit a different surface's saved rank.
Previously unobserved live windows receive a provider fallback position until
actual focus is observed; past focus during an observer outage is not invented.

Opening the application switcher captures the selected native backing window
only when its presentation group has an exact bijection between live QObjects
and broker captures. An unsupported member keeps that group's original QObject
selection. An unrelated unsupported group does not disable an exact native group;
unsupported source focus still uses the original QObject path. App identity alone
cannot join a QObject to a native surface.

One immutable broker observation carries captures, source focus and a bounded
inventory witness. The domain-separated inventory includes all managed foreign
views, including unsupported and owned views, their raw application/X11 facts,
parent/modal policy inputs, and actual/registered process identities and advertised
application IDs. Titles, focus and geometry are excluded. It is bounded to 1,024
views and 1 MiB. The observer nonce, socket epoch and revision are informational
scope fields, not authentication. The numeric revision stops at the largest exact
JavaScript integer; exhaustion disables native observations until restart.

Commit requires both current canonical inventory equality and an unchanged
observer revision. New, close, application-ID and relationship events advance the
revision, so an observed change followed by restoration cannot revive an old
preview. Unobserved transients between samples are not claimed to be detectable.
The compositor's atomic focus guard separately validates the fresh plan at
dispatch. Neither path can expand the captured target/process members.

Before every buffered native request, the observer drains already queued event
frames, including ordinary focus/title prefixes before relevant changes. It
rechecks after GET_TREE and repeats a fresh barrier at most three times under
a five-second compositor sampling deadline. Event batches have a 32-frame,
256-KiB and 50-ms read budget, checked before payload allocation; processing
preserves per-event refresh/focus ordering with a 250-ms compositor-I/O deadline.
GET_TREE partial writes and reads share an absolute deadline. Incomplete frames,
budget exhaustion or invalid framing retire the observer before native dispatch.
These are communication bounds, not a real-time scheduling or local filesystem
latency guarantee. Changes arriving after the final event barrier remain in the
sampled boundary; the compositor guard validates the actual fresh topology.

Presentation grouping, label and exact QObject/capture are frozen for an
interaction. Desktop-entry alias edits take effect on the next invocation;
pending native dispatch does not resolve aliases again.

`tests/ci/window-inventory.nu` runs two finite full-shell inventory scenarios
and the existing nine-group backing-window MRU scenario on both compositor
variants. Its disposable clients prove unrelated unsupported groups, an actual
empty-app-ID source, observed open/close ABA with the same final digest, refusal
without fallback, and a fresh successful retry. Both effects runs require a
software renderer receipt. The original MRU assertions include observer/shell
restart, duplicate captions, closed/replaced targets and owned Calendar sources.
Actual QML command tests separately cover twenty exact-pair, alias-freezing and
malformed-observation cases. A real broker process with a fake IPC provider
tests prefixed queued events, multiple buffered requests, events at the tree
barrier, incomplete frames and no extra dispatch; it does not replace native
compositor acceptance.

Artifacts under `artifacts/native-window-inventory-*` retain exact immutable
pins, raw logs, decisions and bounded diagnostic receipts. The fixture has no
host audio graph, audio server or Bluetooth service. It permits at most one
exact unavailable-service diagnostic for each of those services per desktop
process, records them, and rejects other warnings/errors. Its cached-only
network helper rejects refresh and performs no probes. These intentional
runtime service absences do not relax warnings-as-errors in builds or assert
hardware-service parity.

Its app icon describes an application group; it does not pretend to
show an authenticated window thumbnail. Commit uses a native broker that rereads
the owned compositor socket, checks the source focus and exact captured target,
then applies the shared modal, hidden, minimized and workspace policy. Closing
the target or replacing its process/surface cancels the old decision rather than
selecting a sibling or new window. Application-group unhiding is limited to
captured process-group members; a new process advertising the same application
ID cannot join that old group request. Protocol-linked modal descendants are
validated as live native
surfaces before redirection.

Source focus distinguishes a positively observed empty desktop from unsupported
or unknown focused clients. Unsupported sources fail closed. Owned native shell
panels have source-only process/surface witnesses; they are not promoted to
external application ownership or activation targets.
An independently running native surface can be a positively witnessed source
without becoming a registered activation target. An empty application ID, legacy
source or unknown identity remains unsupported.

Native requests have exact reply ownership and a 30-second total transport
budget. A malformed, unavailable, mismatched, exited or timed-out worker cancels
the owning request with an explicitly uncertain result. That owned worker is
retired and reaped before a replacement can become available. A fresh user
decision is required; no pending action is replayed, retried or rolled back.

The observer repeats live kernel-process checks against its sampled tree, then
submits a [versioned typed guard](native-focus-guard.md). The compositor checks
actual source/target surfaces and the complete canonical policy topology digest
inside one callback before applying the approved actions synchronously. This
closes the surface/source IPC interval; kernel process lifetime remains a
separate sampled witness and can end immediately after its last check. Missing
guard capability keeps the original window-based path and explicitly refuses a
captured native request.

The original helper uses the same policy core and retains its existing API.
Unsupported surfaces, missing foreign identifiers and window-only providers keep
the existing live-window fallback. Those paths do not gain durable window MRU.
Window cycling still uses live creation order. Pending selections, QObject
references, translated titles, commands and document contents are not saved.

The disposable native GTK test exercises two in-memory dirty documents with app-
owned cancel, save and discard handlers. It also restarts the observer and then
the whole isolated shell while all app
windows are closed, checking fresh process validation and retained Dock/switcher
activation. It proves routing and non-forced termination, not the document fidelity
of every retained application.

The separate three-application `mru` scenario checks relative order, an observed
zero-window application, observer and full-shell restart, new focus precedence,
physical preview/Escape/commit and stale retained-owner refusal. It uses native
container and process identities rather than title-based identity inference.
The separate `window-mru` scenario uses two same-process windows with identical
captions, observer/full-shell restart, physical preview/commit, closed-target
refusal and same-process replacement refusal. The distinct diagnostic
`window-mru-baseline` exercises the previous title-only path; its expected failure
is not acceptance of the native adapter.
