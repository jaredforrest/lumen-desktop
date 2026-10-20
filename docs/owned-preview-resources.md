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
Limits are 64 MiB per file and 128 MiB per worker; exhaustion is unavailable with
no original URI fallback. Published resources remain until that worker retires.

Hiding Quick Look or deactivating Files clears its result and decoder bindings
before stopping the resource owner. Unexpected exit clears the result; a new
explicit request can restart. A ten-second request deadline kills only that owned
preview worker and does not retry the stale request. A later worker cleans only
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
holds only actual Image/MediaPlayer source activation, replaces the source after
the real reply, and verifies original image pixels and video duration. It also
checks physical Escape cleanup, reopening, preview-only crash/reconnect,
SIGSTOP/deadline with usable folder controls, oversized-resource refusal, and actual PDF rendering after shared-cache pruning.
Instrumentation digests, original replies, exact identities, screenshots and
strict logs are retained with the fixture result.
