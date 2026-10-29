# Local document history

The pinned document provider exposes an explicit local-history capability through
File → Info → Version History and `file.history`. The existing session-only
`file.versions` interface is unchanged. Other office providers do not acquire
persistent history from this integration.

The user captures a full in-memory document snapshot, chooses a stable version
identity, previews up to 2,000 characters, and explicitly confirms restoration.
Restore requires the captured document identity and revision. Dirty working
content needs explicit replacement consent. Before modifying the working
session, restore records a `Before restore` version; the normal undo checkpoint
also preserves the replaced working document. Restored content remains dirty.
Neither capture nor restoration rewrites the saved source document.

History is tied to the canonical UTF-8 path of a regular saved file. A private
`.filename.lumen-history` directory beside that file contains a mode-0600 archive
and advisory lock, inside a mode-0700 directory. Unsupported file aliases,
special files, nonprivate state, incompatible schemas, invalid version identities,
concurrent owners and changed archive entries fail visibly. Source-folder moves
and renames do not migrate history; include the history folder in backups.

Archives retain at most 20 snapshots and 32 MiB, expiring oldest snapshots when
needed. A single oversized snapshot refuses without replacing the existing
archive. Publication uses the shared captured-entry atomic writer and a directory
sync. This is explicit capture, not automatic backup, cloud history, an atomic
filesystem snapshot or a hostile same-user concurrency/power-loss guarantee.

A protected document capture encrypts the whole archive using the provider's
existing authenticated encryption implementation, including earlier snapshots
and labels. Reading older protected history may require its capture-time password
after the document password changes. Capturing without a current document
password preserves existing archive encryption. Old unprotected history remains
unprotected until explicitly recaptured with protection; the UI discloses this.
History password fields and captured owner facts are excluded from serialized
public dialog state. No secure-erasure or physical-storage retroactivity is
claimed.

The Rust `word-durable-history` fixture exercises the installed provider through disposable
stdio sessions, including process reopen, preview, consent, invalid/stale restore,
undo, busy/special entries, archive protection and source-byte preservation. Its
separate `--capacity-probe` checks byte-based eviction, oversized capture refusal,
oversized pre-restore recovery refusal and oversized archive reads. Rust source
tests separately cover count retention, permissions, changed targets, captured
UI ownership and stable decision controls with long recovery content.

The Rust `office-native-workflows --app writer --history-probe` fixture exercises actual
provider-owned Capture, selection/preview, Cancel, stale held Restore, confirmed
Restore and independent GUI process reopen on both compositors. These bounded
provider tests do not establish external application, cloud or visual reference
parity; headless tests alone do not establish physical UI acceptance.
