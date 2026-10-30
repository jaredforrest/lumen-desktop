# Presentation local history

The presentation provider offers **File → Version History…** for a saved regular document. Capture version stores the full working presentation, including unsaved edits, speaker notes, presentation settings, masters, round-trip extension data and embedded media bytes. It does not save or replace the source file.

Versions are explicit local captures, not automatic backups, synchronization or system-wide versioning. Keep the hidden history folder beside the source with backups. Renaming or moving the source does not migrate its path-bound history. The archive retains at most 20 versions and 32 MiB; oldest captures expire as needed. A single oversized snapshot is refused.

An optional history password encrypts the entire archive at capture. It protects the archive only: the presentation provider does not implement source-document password protection. Enter the existing archive password to browse or capture an encrypted archive. Capturing without a new password preserves existing archive protection. Passwords are excluded from UI serialization, debug output and the engine command journal.

Restore selected version opens an explicit confirmation. Cancel leaves the working presentation unchanged. Confirm requires the captured document identity, revision and path to remain current. Dirty presentations require explicit consent. Before changing the working presentation, the provider publishes a **Before restore** snapshot; a publication failure leaves the working presentation unchanged. Restoration remains unsaved, and Undo restores the previous working presentation. The source on disk changes only after a later explicit save.

The preview is a bounded outline and speaker-note excerpt. It is not a rendering of every slide and does not define snapshot fidelity: the stored snapshot includes the complete semantic presentation plus embedded binary media.
