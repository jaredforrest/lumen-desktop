# Local workbook history

The pinned spreadsheet provider exposes File → Version History and `file.history`.
Explicit snapshots preserve the semantic workbook, including formulas, styles,
charts, sheet metadata and binary pictures. Derived formula, spill and style
caches are rebuilt on restoration; this recalculates formulas.

Capture does not save the working document or change its dirty state. Restore
requires the captured workbook identity and revision, with explicit consent for
unsaved changes. A full `Before restore` snapshot is published before replacing
the working workbook. The restored workbook remains dirty and normal Undo
preserves the replaced workbook and selection. The saved source stays unchanged.

The optional history password protects only the local archive. It does not add
encrypted workbook-file support. A protected archive needs its existing history
password; omission or a wrong password refuses without changing the document.
Capturing with a password encrypts earlier snapshots and labels as well. Existing
archive protection is retained; the interface does not offer password removal.
Credentials are excluded from serialized dialog state and the command journal.

The shared archive crate enforces private ownership/modes, regular canonical
UTF-8 source paths, advisory locking, captured-entry publication and the limits
of 20 versions and 32 MiB. Oversized recovery capture refuses before restoration,
so failure cannot silently discard the current unsaved workbook. See
[the shared archive contract](document-history-archive.md) for persistence limits.

This capability is explicit local history for this provider. It does not claim
automatic backup, cloud history or equivalent support in external applications.
