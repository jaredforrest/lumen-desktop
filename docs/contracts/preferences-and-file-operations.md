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

### Recoverable Copy Undo

Eligible Copy and Duplicate outputs enter the same per-item, last-20 Undo stack
as Move. Copy labels identify the actual created destination, including keep-both
names. A partial batch records only outputs that were successfully published.
Undo capability is session-local and is not reconstructed after service restart.
An ineligible copy still completes successfully and reports a separate
`undoWarning`; any earlier eligible item remains available by its named label.
The status API exposes `undoKind` and `undoLabel` alongside `canUndo`.

Copy Undo moves only the unchanged captured destination into the user's personal
Trash on the same volume. Its source is untouched. The existing Put Back action
restores the trashed output without overwriting collisions. No recursive
permanent deletion or automatic cleanup is used. Cross-volume Trash, replaced
parents or outputs, modified contents, new descendants, mounted contents,
special files and inaccessible no-atime inspection all refuse safely.

Each manifest is limited to 4,096 entries, 64 descendant levels, 64 MiB of regular
file content and 1 MiB of encoded manifest data. Hashing a growing file reads at
most the captured content size plus one detection byte before refusal. Each
inspection checks a cooperative two-second deadline between operations; this
does not impose a hard deadline on a blocking filesystem read. Pending batch
records are evicted above 20 immediately, as is the retained final Undo stack;
at most 20 prior and 20 pending manifests coexist during a batch. The manifest
bound is an encoded-data bound, not a total allocator-memory measurement.
Cancellation is checked at Undo admission and again immediately before claiming
the item. Once claimed, recovery completion and reporting continue even if a
cancel arrives; cancellation never suppresses recovery instructions.

Undo saves a recovery journal entry before claiming a destination. A private
0700 staging directory contains recovery instructions until normal Trash
publication is durable and recorded. On a post-claim race, Undo attempts a
no-overwrite return to the original slot. If that slot is occupied, or Trash
publication partially succeeds, it retains the payload and reports the current
reachable recovery path. If that path cannot be verified, the error explicitly
says so; it does not claim the item remained at its original destination.
Retained staging is never silently expired or retried. Follow its recovery
record, or use Put Back when an ordinary Trash entry is available. Existing
Move Undo behavior is unchanged.
Inspection samples inode, owner/group, mode, content, timestamps and descendants;
rename comparison permits the root ctime change caused by the rename itself.
This is not an atomic tree lock or a complete ACL/arbitrary-xattr witness.
Concurrent root-only extended-metadata changes inside that rename seam are not
independently distinguished. Payloads are always retained recoverably.

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


## Supported owned appearance and layout controls

The typed adapter now accepts `appearance.increaseContrast`,
`appearance.sidebarIconSize` (`small`, `medium`, `large`) and the additional
`when_scrolling` scrollbar mode. Increased contrast strengthens semantic text
and boundaries and uses opaque materials. Sidebar sizes reflow the owned Files
and Settings rows. These options do not rewrite retained applications.

Dock preferences include unique desktop-entry IDs in `dockPins` (null restores
the shipped defaults), `dockRecentApps`, `dockIndicators`, `dockPosition`,
`dockAutoHide`, `dockSize` (32–72), and unique absolute `dockFolders` with typed
sort/order/presentation options. `windows.tileMargins` and
`windows.titlebarDoubleClick` apply only when the compositor advertises
`window_placement_preferences`; the Settings response discloses when a new
login is required. Preferred-content zoom is not advertised as Fill.

`controlCentre.order` is a complete permutation of the registered control IDs;
`controlCentre.hidden` is a unique subset. Unknown, duplicate or omitted order
IDs are rejected. `notifications.preview` supports `always` and `never`;
authenticated lock-dependent preview policy is not offered without lock-state
authority. `search.disabledProviders` is a unique subset of applications, files,
actions and clipboard. Disabling a Search provider excludes its results and
navigation, but does not revoke independent clipboard collection consent.
Settings offers explicit Clear and Disable Clipboard History actions.

## Recent queries

`search.queryHistory` defaults to false. When enabled, dismissing ordinary
Search saves the nonempty query; Clipboard-category queries are excluded.
Alt+Up and Alt+Down recall the last 20 distinct queries and return to the draft.
Clear Search History removes saved terms; disabling history removes saved terms
and refuses further collection. The history store is an owned private 0700
directory with 0600 files, excludes control characters and overlong queries,
and never sends query contents in process arguments. Clearing history does not
clear the filename index or clipboard history.

## Files views and metadata

Files saves view, sort, descending order, folders-first, hidden-file visibility,
kind grouping, and sidebar destinations as application defaults. Existing open
windows keep their current view until refreshed or reopened. Kind grouping has
visible sections in list view. Sidebar destinations can be added, reordered or
removed without changing any underlying files. The destination context menu is
also available through the Menu key or Shift+F10.

Duplicate uses the normal no-overwrite copy job with keep-both naming. Make
Symbolic Link is an explicitly labelled platform substitute: it records a path,
shows a link badge, and does not automatically track a renamed target. A broken
link produces a recoverable error with File Info guidance. Open With marks the
current association and changes it only through the explicit Set Default action.
Other opening paths continue to use the shared system association.

Seven standard colour tags have stable IDs, labels and colours, stored in
`user.lumen.tags.v1` metadata independently of file names. Multiple tags can be
applied and searched using `tag:red tag:blue` in Files and Search. Unsupported
metadata storage is disclosed. Links are not followed; the editor refuses an
item replaced since it opened. Copies preserve tag metadata, and a destination
that cannot preserve existing tags fails while retaining source data. Custom
named tags and interoperability with other metadata formats remain unsupported.

## Optional desktop file surface

`desktop.showFiles` defaults to false. Enabling it displays the configured XDG
Desktop folder on the primary display, behind application windows. Missing
folders are disclosed and are never created automatically; the user-folder
convention that disables Desktop by pointing it to the home directory is
respected. Desktop sort/order are separate from Files window preferences.

Selection supports pointer, modifier and keyboard navigation. Opening, renaming,
creating folders, clipboard operations and Trash use the existing Files service.
Delete opens confirmation with Cancel focused; Escape or disabling the surface
cannot execute the pending deletion. Confirmed Trash remains recoverable through
Files. External directory changes update the view. The nested compositor test
uses only disposable files and verifies actual pointer selection, physical
Delete/Escape, rename, directory events and confirmed Trash.

Desktop stacks/grouping, drag placement, network locations, arbitrary named tags
and complete reference desktop interactions remain outside this first functional
surface. This implementation is not a claim of complete desktop-file parity.

`appearance.dockFiles` is a unique list of absolute local file paths. Settings
adds and removes these references; removing a Dock reference does not remove its
file. Missing or unsupported targets must report an activation error through the
file-alias adapter rather than launching a folder or silently substituting one.
