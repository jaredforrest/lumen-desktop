# Provider-owned snapshot archives

The spreadsheet and presentation adapters share the Rust 2024
`lumen-document-history` crate. Each adapter serializes and restores its actual
document model. Presentation media needs an explicit byte payload because its
upstream model omits media bytes from ordinary JSON serialization. Workbook
serialization includes picture payloads; restoration rebuilds derived caches.
These are full semantic snapshots rather than exported office-format copies.

Each regular saved UTF-8 document has a neighboring `.filename.lumen-history`
directory (0700) containing an archive and lock (0600). The archive binds schema,
provider and canonical source path. Moving or renaming a source does not migrate
history. Include its history directory in backups. Version identities are stable
and validated; incompatible schemas, aliases, nonprivate entries and concurrent
owners fail visibly.

The common store pins the directory and captured destination entry, avoids
blocking on special files, holds the advisory lock during each operation and
uses the existing shared atomic-publication implementation. The captured named
directory is checked before publication and synchronized afterward. Retention
expires oldest versions at 20 snapshots or 32 MiB. A single oversized snapshot
refuses while preserving the old archive. Candidate changes remain transactional
in memory as well as on disk.

An optional archive password encrypts the complete JSON archive, including its
path, labels and snapshots. The versioned `LUMENHST1` envelope uses AES-256-GCM,
a fresh random 16-byte salt and 12-byte nonce, and an Argon2id v19 key derivation
with fixed parameters (19,456 KiB, two passes, one lane, 32-byte output). The
envelope version is authenticated as associated data. The implementation uses
[RustCrypto AES-GCM](https://docs.rs/aes-gcm/0.10.3/aes_gcm/) and
[Argon2's key-derivation API](https://docs.rs/argon2/0.5.3/argon2/); it does not
implement cryptographic primitives itself. Changing the parameters requires a
new envelope version. Unsupported or modified protected archives refuse.

Retained UI/store passwords, derived keys and temporary archive plaintext bytes
use zeroizing containers. Transient JSON command/transport values use the existing
provider protocol's ordinary memory. The UI's credential type is redacted in
diagnostics and excluded from serialized state; history commands do not enter the
provider command journal. This is not
secure erasure, a hostile same-user guarantee or a power-loss/filesystem snapshot
guarantee. Unprotected archives are plaintext, and later encryption cannot erase
earlier storage copies. Source documents remain under their existing format and
encryption capabilities.

Before restoring, the provider publishes a snapshot of the current working
document. Only then does its normal undoable command replace the in-memory model.
Unsaved replacement requires captured identity/revision and explicit consent.
The provider-owned dialog retires stale and consumed decisions; errors leave a
safe Close action. Its decision footer remains outside scrolling history content.

The existing document provider keeps its established archive format and behavior.
Its shared atomic-publication source is unchanged; no existing archive migration
or cross-provider archive compatibility is claimed.

Native process-reopen checks first verify physical document discard and preserved
source bytes, then retire the owned disposable provider process before starting
an independent process. They do not assert that closing the last document must
exit its application. Each provider's actual history entry point and decision
controls are exercised separately from command-line archive checks.
