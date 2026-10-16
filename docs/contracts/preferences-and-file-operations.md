# Preferences and file operations

## Preference writes

User-facing appearance edits use `lumen-nix-settings patch SECTION JSON_OBJECT`.
The writer reads the latest authoritative values while holding its transaction
lock, merges only existing keys, evaluates the typed Nix schema, and activates
through the existing authenticated deployment workflow. `set` remains a complete
section replacement for intentional bulk updates. Neither operation bypasses
source cleanliness checks, activation authentication or rollback.

Settings queues edits made while a save is running. Pending edits to the same
key coalesce to the latest value; edits to different keys preserve order. The
interface reports an error from a failed operation rather than claiming it saved.
File-view preferences independently reject unknown keys and non-boolean sort
flags before replacing the persisted file.

`appearance.switchToAppSpace` is a boolean defaulting to true. The activation
adapter consumes it; it does not change explicit user selection of a window.

## Operation requests and recovery

File mutation requests may carry a `requestId` of 1 to 128 visible ASCII
characters without spaces. Reuse the same request ID and identical payload after
a transport timeout. A retained request returns its original job, including after
service restart; reuse with another payload is rejected. Newly initiated actions
use new request IDs. The shell transport includes a session-unique ID on copy,
move, trash, restore, undo, paste and drop requests.

This receipt protection currently has the same retention as operation history:
the latest 40 jobs. It is not an indefinite mutation ledger. An outcome older
than that retention must be reconciled by inspecting source/destination before
retry, rather than assuming an old ID still deduplicates it. A recovered running
job is `interrupted`, with an instruction to inspect both locations; restart
does not automatically replay a mutation.

Copies stage privately and publish with no-replace rename. Cross-filesystem
moves flush copied file metadata and directory entries, publish the destination,
and flush its parent before removing any source. A durability failure keeps the
source and discloses the failed operation; a complete destination may also exist.
A changed source aborts before deletion. Cancellation removes unpublished
staging and keeps uncopied source data. No replace or permanent-delete API is
implied. Journaling errors stay visible, and job snapshots cannot mutate history.

## Verification scope

Disposable filesystem tests exercise conflicts, cancellation, changed sources,
dangling links, restrictive directory modes, cross-volume moves, destination
flush failure, undo, trash restoration, duplicate requests and service restart.
The Nix evaluator tests use isolated settings sources, including patch
preservation and rejected writes. These tests do not establish screen-reader
interoperability, document fidelity of retained applications, service integration
or reference visual equivalence. Those require provider-specific and rendered
session evidence. Existing applications retain their supported native document,
security and permission semantics.
