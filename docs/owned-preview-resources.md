# Owned preview resources

Files and Quick Look use `OwnedPreviewState.qml` and the dedicated persistent
`owned_preview.py` worker. Folder listings and file operations have a separate
process. Each request captures a sequence, source path, page, selected device and
inode when available, and an observed source token. Paging retains that token;
explicit selection/show observes a fresh source. Replies from retired or replaced
requests cannot publish a resource.

Regular source files are copied through the shared `PreviewSnapshots` descriptor
validation API before extraction or rendering. Image and media decoders receive
private immutable resource URIs; rendered PDF cache images are retained too, so
shared cache pruning cannot invalidate a published page. Displayed names and
metadata describe the validated original source, not its temporary resource.
Copy limits are 64 MiB per file and 128 MiB per worker; supported renderers retain
their existing narrower limits, including 32 MiB for inline image display; exhaustion is unavailable with
no original URI fallback. The budget now counts a working set rather than every
past selection: source and result/page leases share references to immutable
resources. Unchanged-source PDF paging retains its original input while replacing
the previous page raster. A new selection retires both old input and result.
Every new snapshot has a fresh pathname; retired URIs are never reused.

`PreviewRequests` registers a request before dispatch and scopes release messages
to the provider generation. `PreviewSnapshots` pins resources during capture and
render, so cancellation before dispatch cannot resurrect a lease and retirement
cannot unlink an input while a worker still uses it. Search has a separate serial
preview lane; filename queries and external-open actions keep their existing
execution path. The next preview waits for the previous retirement acknowledgement.

The UI first clears its result. `PreviewResourceConsumer` observes each actual
Image source and each MediaPlayer source/stopped state; Files registers both its
gallery and sidebar consumers. `PreviewResourceRetirement` acknowledges only once
all old sources are cleared and old media is stopped, or the actual source owner
is destroyed. Pending acknowledgements retain the exact Image/MediaPlayer QObject
references: removing or rebinding a tracking adapter alone is not retirement.
A later Qt turn schedules this observation and is not itself an acknowledgement.
This is a cooperative source-retirement contract, not a claim that an internal
decoder thread has completed. Unacknowledged sources remain leased and block new
allocation rather than being evicted.

Hiding Quick Look or deactivating Files clears its result and decoder bindings
before stopping the resource owner. Unexpected exit clears the result; a new
explicit request can restart. A ten-second operation deadline clears the result and retires only that owned
preview worker after cooperating consumers acknowledge source retirement; a
blocked consumer keeps its old resource pinned. A new capture starts its own
request deadline after the retirement phase, and no stale request is retried. A later worker cleans only
dead process/start-identity snapshot directories, preserving other live owners.

The older `files.py` preview endpoint and `file_preview.preview` remain read-only
helpers; their raw path-based response is not the owned UI resource-lifetime API.
Folder listings and external Open/OCR/subject-extraction actions do not acquire
this immutable-reader guarantee. This batch does not claim immutable external
application handoff or cross-reboot identity.

`test_owned_preview.py` checks source metadata, exact selected identity, symlink
refusal, provider token/page scope, resource bounds, independent owners and
rendered-cache retention. `owned_preview_input.py` uses installed components on
both compositors, private synthetic directories/media and a private bus. It
holds actual Image/MediaPlayer source activation and cooperative source retirement, replaces the source after
the real reply, and verifies original image pixels and video duration. It also
checks physical Escape cleanup, reopening, preview-only crash/reconnect,
SIGSTOP/deadline with usable folder controls, oversized-resource refusal, and actual PDF rendering after shared-cache pruning.
Instrumentation digests, original replies, exact identities, screenshots and
strict logs are retained with the fixture result.

The working-set tests also copy more than a lifetime budget of real inputs,
observe bounded current resources, render multiple PDF pages, and hold actual
image/playing-media sources across selection changes. Existing original-path
replacement refusal on a later PDF page remains conservative: unchanged-source
paging reuses the immutable input, but replacement makes the next page request
unavailable. Continuing old-source paging after replacement is a separate follow-up.

## Captured content and application actions

The preview provider issues an opaque action capability for the selected source
version. It is valid only in that provider generation and retires with its
preview request. Quick Look deliberately permits repeated explicit Open or
content actions while that capability remains current; it never retries an
external Open automatically. A failed action has a separate response and does
not replace a successful preview. Unsupported or oversized preview content can
still have a captured Open/Open With capability.

Application enumeration returns a separate chooser generation and captures the
application registration's pathname, resolved target and version facts. Choosing
an application requires that generation, application ID and selected source to
remain current. Files consumes its chooser capability after an attempted action;
closing its chooser retires it. Buttons capture resource, provider, chooser and
application identity on press, so delegate reuse cannot select another app.
Quick Look clears its chooser intent when the chooser closes.

These are predispatch checks. External file handlers and `gio launch` may reopen
source or application-registration pathnames after the handoff. The broker does
not claim to preserve those external reads through an immutable descriptor.

OCR and extraction instead consume the retained immutable source, never the
original mutable pathname. One owned action worker pins a dedicated source lease
until the fixed helper completes or is canceled and reaped. Exact action ID,
capability and provider generation scope cancellation. Explicit Cancel keeps the
visible preview and its lease; replacing/dismissing the preview waits for actual
consumer retirement before stopping the whole provider. Teardown signals the
worker and joins it before removing snapshots. The content supervisor gives the
existing helpers 75 seconds total and the UI waits 80 seconds; these are separate
from the unchanged 8-second renderer guard and 9-second renderer supervisor.
OS-uninterruptible I/O can delay process termination.

Extracted-subject export captures the output's version facts. The helper opens
only a proven regular descriptor and creates a bounded anonymous copy before
clipboard/export use; a replaced or mutated output is refused. The Save dialog
retains its own output and originating preview identity, and late feedback cannot
attach to another selection.

Unavailable responses distinguish `unsupported`, `policy_limit`,
`source_changed`, `permission_denied`, `application_unavailable`,
`provider_timeout`, `provider_failure`, `dispatch_failed`, `dispatch_unknown`, `canceled`, `busy` and invalid requests.
An Open acknowledgement timeout is unknown and is never retried automatically. A known content/preview Python timeout has the timeout reason; an ambiguous child termination,
including status 137, remains a provider failure. Neither policy limits nor
unsupported formats imply that the original was opened. The 64 MiB source bound,
32 MiB inline-image bound and conservative changed-original PDF paging refusal
remain in place.
