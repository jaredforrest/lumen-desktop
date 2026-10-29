# Share destinations

The owned Files Share action opens a destination chooser. Selecting a row does
not invoke it. Open Destination explicitly authorizes the captured selection;
Cancel closes the chooser without invoking any destination. Offline and
unavailable entries remain inspectable with actionable details and Refresh.
Nothing is sent automatically when the menu opens, the selection changes, a
provider returns, or connectivity changes. An empty registry is an explicit
configuration state.

## Consumer boundary

Consumers may supply `services.lumen-desktop.integrations.sharing.providers`
entries with a stable `id`, fallback `label` and `providerPackage`. The package
must expose `bin/lumen-share-provider`. Up to four providers, including the
optional existing nearby adapter, are supported. IDs are unique lowercase
identifiers; `nearby` belongs to that adapter when it is configured. Packages,
configuration manifests and executable identities resolve to immutable store
paths. Lumen does not install a receiver, choose an account, enable a service,
change firewall/network policy or select a recipient.

The generated `LUMEN_SHARE_DESTINATIONS` manifest is an internal launcher
interface. Provider advertisements cannot supply an executable, argv, shell
fragment or alternate endpoint. Invocations use literal argument arrays and a
bounded JSON pipe, never a shell. A provider endpoint is trusted consumer code;
it must perform its own recipient permission and final file revalidation before
committing a transfer.

## Endpoint protocol

`lumen-share-provider share-describe` returns one JSON object on stdout:

```json
{"schema":1,"generation":"routing-epoch-1","label":"Example destination","state":"ready","detail":"Choose a recipient in the provider","actions":["files"]}
```

`generation` is a non-secret opaque token, at most 128 characters, stable for
the currently advertised account/route/provider instance. It changes whenever
that routing identity changes; it is not a timestamp or a new token per read.
Its exact value participates in the captured destination fingerprint.
`state` is `ready`, `offline` or `unavailable`. Readiness is provider-advertised;
the shell performs no Internet probe or implied account check. Labels are at
most 80 characters, details at most 160, with no control characters. Only the
`files` action is supported. Unknown fields, duplicate JSON keys, diagnostics,
malformed replies and nonzero status fail closed. Description output is bounded
to 64 KiB and 750 ms per endpoint; the registry has at most four endpoints.

After explicit confirmation, `share-files` receives JSON on stdin:

```json
{"schema":1,"files":[{"path":"/absolute/example.txt","device":"4","inode":"8"}]}
```

Device and inode values are canonical unsigned 64-bit decimal strings, never
rounded numeric identities. Selection is limited to 128 unique regular files.
The backend refuses symlink replacements, changed identities, relative paths,
NUL and excessive requests before invoking the endpoint. A fingerprint of the
configured endpoint and its complete advertisement must still match a fresh
read. A provider that became offline cannot receive the files request.

The endpoint returns exactly `schema`, `state` and `detail`. Outcome states are
`completed`, `cancelled`, `offline`, `error` and `opened`. The UI attributes
completion to the provider; it does not independently claim remote delivery.
A pending invocation remains visible and cannot falsely report cancellation.
Complete or cancel in the provider UI. Invocation pipes are byte bounded and
the owned process group is retired after the 120-second protocol deadline. A
missing or invalid outcome reports unconfirmed delivery and requires checking
the provider before retrying. Interruption retires and reaps the owned endpoint
group before reporting an unconfirmed outcome. There is no automatic retry.

## Existing nearby adapter

The existing nearby endpoint and transport code are unchanged. Its real status
advertises the destination; its generation pins the immutable configured
endpoint. It cannot invent a daemon/account generation absent from that legacy
protocol. Missing send capability is unavailable, not an inferred network
offline state. The adapter rechecks fresh status and captured file
identities. Explicit Open Destination then hands off through the existing
application launch route, preserving its recipient UI, lifetime and consent.
The shell says to complete sharing there and marks delivery unconfirmed. It
does not impose the new protocol deadline on that interactive transport or
infer Cancel or delivered status from an unstructured exit code. Non-UTF-8
filenames are refused before this legacy text-argv handoff rather than changed
by the toolkit. The JSON endpoint protocol preserves those escaped filenames.

## Limits and verification

Captured context includes folder, tab, paths and exact file identities. A later
selection, tab, folder or replaced file cannot redirect the captured choice.
These cooperative checks are not a hostile same-user or atomic path-to-transfer
guarantee. In-place content edits do not create a new filesystem identity.
Providers must revalidate the payload they actually commit. This integration
shares selected saved Files entries; it does not invent an unsaved document
snapshot or add share support inside retained third-party applications.

`res/lumen-desktop/share-destinations/tests/registry.rs` exercises real private synthetic endpoint processes,
actual JSON requests, offline changes, Cancel/outcomes, changed advertisements,
replaced/symlink files, malformed/flooding/diagnostic endpoints, nonreading input
inherited-pipe descendants, interruption during an actual provider wait and
restoration of the caller's process-reaping role. No external service or real recipient is used.
`lumen-share-destinations-fixture` exercises the actual owned chooser and process state
with a recorded private synthetic endpoint in the full disposable software
boundary. It covers explicit Cancel, stale selection, pending feedback, offline,
Cancel/error/handoff outcomes, enlarged constrained focus/paint, and literal
arguments. It makes no native pointer, delivery or reference visual parity claim.
`lumen-files-share-menu-fixture` separately checks actual Files menu capture and changed
selection/inode refusal before opening the chooser.
