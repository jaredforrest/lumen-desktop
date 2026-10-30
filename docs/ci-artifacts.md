# CI artifact snapshots

The integration upload uses `tests/ci/backend-artifact-paths.txt` as its single
include policy. The source contract checks that declared fixture outputs remain
covered, and the staging package validates the actual policy during its build.

`lumen-stage-artifacts` copies selected regular files into a new upload tree,
preserving repository-relative paths, file bytes and permissions. It leaves the
original fixture outputs untouched. Its manifest records every omitted path and
its exact filesystem type. Symlinks, sockets, FIFOs and devices are never copied
or dereferenced. Descriptor-relative traversal and no-follow opens keep parent
replacement and leaf symlink replacement from redirecting reads.

The snapshot is published by rename only after completion. The limits are100,000
files, two GiB including the manifests,200,000 enumerated entries and128 directory
levels. An exceeded budget fails staging clearly and removes its incomplete
temporary tree. File copies stop after at most the remaining byte budget plus
one detection byte, including when a source grows. This is a bounded diagnostic
snapshot of normally quiescent test outputs, not an atomic snapshot of changing
file contents or a general untrusted-filesystem security boundary.

Real Unix-socket and external/dangling/loop symlink fixtures verify omission,
exact byte and mode preservation, policy exclusions and unchanged originals.
Further tests cover replacement by a symlink, captured parent directory identity,
copy bounds and budget failure without publishing partial evidence.

The observed hosted upload finalized successfully but emitted four
`ENTRYNOTSUPPORTED` warnings without entry names. The snapshot prevents these
unsupported filesystem types from reaching the ZIP uploader; the exact omitted
hosted paths are recorded on its next run, rather than inferred from that log.
