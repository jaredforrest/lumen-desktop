# CI artifact snapshots

The integration upload uses `tests/ci/backend-artifact-paths.txt` as its single
include policy. The source contract checks that declared fixture outputs remain
covered, and the staging package validates the actual policy during its build.

`lumen-stage-artifacts` copies selected regular files into a new upload tree,
preserving repository-relative paths, file bytes and permissions. It leaves the
original fixture outputs untouched. Its manifest records each selected special
entry's path and exact filesystem type; the include policy is retained separately.
Symlinks, sockets, FIFOs and devices are never copied
or dereferenced. Descriptor-relative traversal and no-follow opens keep parent
replacement and leaf symlink replacement from redirecting reads.

The snapshot is published by rename only after completion. The limits are 100,000
files, two GiB including the manifests, 200,000 enumerated entries and a traversal
depth of 128 below each selected root. An exceeded budget fails staging clearly
and removes its incomplete
temporary tree. File copies stop after at most the remaining byte budget plus
one detection byte, including when a source grows. This is a bounded diagnostic
snapshot of normally quiescent test outputs, not an atomic snapshot of changing
file contents or a general untrusted-filesystem security boundary.
Selecting no regular fixture files is an error and publishes no upload tree;
the two metadata files cannot make missing evidence look successful.

Real Unix-socket and external/dangling/loop symlink fixtures verify omission,
exact byte and mode preservation, policy exclusions and unchanged originals.
Further tests cover replacement by a symlink, captured parent directory identity,
copy bounds and budget failure without publishing partial evidence.

The observed hosted upload finalized successfully but emitted four
`ENTRYNOTSUPPORTED` warnings without entry names. The snapshot prevents these
unsupported filesystem types from reaching the ZIP uploader; the exact omitted
hosted paths are recorded on its next run, rather than inferred from that log.
