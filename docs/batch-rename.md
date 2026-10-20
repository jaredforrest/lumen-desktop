# Captured batch rename

Selecting multiple siblings and choosing Rename opens a separate preview. Replace
text, add prefix and add suffix transform complete names, including extensions.
Every before/after name and conflict is shown. The default stops before changes
if any conflict exists; an explicit Skip Conflicts choice permits unaffected
rows. Invalid names or changed source identities always block the whole preview.
Existing destinations are never overwritten, including dangling links and names
belonging to another selected row. Rename cycles are not staged through temporary
names. The existing single-item Rename remains available.

The service holds an opaque, one-use plan for five suspend-aware minutes. It
captures exact native metadata, the parent identity, policy and transformation;
confirmation sends only its token and an explicit confirmation flag. Editing,
closing or disconnecting removes dialog authority. Late preview responses retire
their token. A provider restart cannot reconstruct plans from paths. No plan or
selection is persisted. Limits match the 5,000-row Files listing, with at most64
simultaneously retained previews.

Before writing, the helper rechecks every source and destination. It uses a
retained directory descriptor and atomic `renameat2(RENAME_NOREPLACE)` for each
item, then checks the resulting object. A known hard-link ctime change caused by
an earlier row is reconciled without accepting content/mode/mtime changes. It
stops at the first error and reports renamed, failed, not-run, unchanged or
explicitly skipped rows. Completed changes stay committed: no automatic retry or
rollback can overwrite a name another process subsequently created. A failed
folder durability acknowledgement is reported even when the renames completed.

Linux offers no atomic rename conditioned on a source inode. Another writer can
replace a source between the last identity check and the rename syscall. The
helper detects a different resulting identity and stops remaining rows, but
cannot undo an independently changed namespace safely. It does not claim a
filesystem-wide transaction or isolation from unrelated writers. No-overwrite
is enforced atomically at the destination.

`test_batch_rename.py` uses actual disposable filesystem objects for Unicode,
case-only changes, no-ops, collisions, stale sources/parents, expiry, hard links,
symlinks, real permission refusal, partial results and racing destinations.
`test_batch_rename_qml.py` loads the actual dialog offscreen with a synthetic
transport to test captured selection, one-shot consent, late replies, editing,
partial results and disconnect retirement. Native pointer/keyboard acceptance is
a separate gate; those model/protocol tests alone do not prove it.
