# Responsive settings adapters

A consumer can opt into saved-first transactions by setting
`LUMEN_NIX_SETTINGS_ASYNC=1` together with its existing authoritative Git adapter.
The runtime still validates the complete candidate with the Nix settings schema,
checks concurrent edits and commits the source before acknowledging the save.
A private projection at `$XDG_STATE_HOME/lumen-desktop/settings/current.json`
contains the validated values and exact source bytes. Readers use it only while
those bytes still match the authoritative source. It is a regenerable projection,
not a second source of configuration policy; consumers with ephemeral homes must
persist this directory or recreate the projection from the committed source.

The deployment adapter must implement `queue` and `propagation-status`.
`queue` starts one independent background worker and returns promptly.
`propagation-status` returns a state (`idle`, `pending`, `building`, `authorizing`,
`syncing`, `applied`, `sync-pending` or `failed`), revision and optional `message`.
The Settings interface polls that status and provides explicit retry. Deployment
failure keeps the committed preference; it must not roll back an acknowledged
save. Worker startup failure must remain visible and retryable too.

The consumer owns guarded deployment and publishing. It must serialize workers,
limit resource use, reject obsolete candidates, retain administrator authorization
and native system checks, and publish only the exact applied revision. A new save
must never wait on a running system build or its authorization prompt. Unsupported
immediate application can finish during background system propagation.
