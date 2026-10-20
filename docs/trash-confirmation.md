# Confirmed personal-Trash deletion

Files exposes **Delete Permanently…**, **Empty Trash…** and **Trash Results…** only in personal Trash. Existing move-to-Trash and Put Back remain separate recoverable operations. There is no permanent-delete fallback for arbitrary paths, network locations or unsupported Trash entries.

Preparation captures an immutable manifest of the selected already-trashed objects (or the current personal-Trash contents), native identities, paired recovery records and descendant facts. The dialog lists the captured names/count and irreversible consequence. Cancel receives initial/default focus; Escape retires the confirmation. Confirmation submits an opaque session-only token, never a new path list. New arrivals are excluded. Replacement, changed contents, unsupported special files or nested mounts prevent deletion of that captured item. Symbolic links are leaves; their targets are never traversed.

The existing per-user file-operation service owns accepted work independently of a window/client. Confirming twice returns the retained result without re-running deletion. Pending confirmations expire after 120 seconds and never survive a service restart. No interrupted operation is automatically replayed.

## Bounds and outcomes

A preparation accepts at most 1,000 items,4,096 total nodes,8 GiB aggregate regular-file stat sizes,64 descendant levels and 4 MiB of manifest metadata. Initial public result metadata is capped at 1 MiB. Directory enumeration is bounded before accumulating unbounded names. The preparation budget is 2 seconds and deletion budget 5 seconds, checked cooperatively between filesystem operations; these are not hard deadlines for a blocking kernel/filesystem syscall. Over-limit preparation fails before mutation. The journal is capped at 32 MiB; up to 40 operations are retained, and incomplete operations are never silently evicted to admit another deletion.

Before irreversible deletion, each unchanged item is staged without destination replacement in a private recovery directory and its journal is atomically written and fsynced. Relevant parent directories are fsynced. Unsupported or failed durability acknowledgement is an error, not success. No API is claimed to provide inode-conditional rename/unlink: descriptor-relative no-follow traversal, exact pre/post validation and controlled quarantine constrain races, while later arrivals are preserved or cause an explicit partial result.

Results disclose each item's deleted/kept/partial/interrupted state, removed-node count, errors and remaining payload/recovery-record/recovery-directory paths. Stop Remaining is cooperative and preserves what has not been removed. An untouched staged object is rolled back only without overwriting a later arrival; otherwise it remains at the disclosed recovery location. A partly deleted directory retains its remaining captured/new contents and recovery record. Recovery is an explicit manual review, never an automatic destructive retry. Retained results remain inspectable after restart, including interrupted/uncertain states.

## Consumer persistence boundary

The consumer must persist its personal `$XDG_DATA_HOME/Trash` (normally `~/.local/share/Trash`) for recoverability across reboot. This includes `files`, `info` and `.lumen-deletion`; there is no additional durable state path. `.lumen-deletion` is 0700 and `journal.json` is 0600. Root/recovery replacement, symbolic-link redirection, insecure ownership/permissions or separate mounts are rejected. The desktop does not configure host storage, security or backup policy. Consumers are responsible for protecting/persisting their Trash; no public asset or private host configuration is bundled here.

## Reproduce disposable acceptance

Run `nix develop -c desktop-test-python tests/desktop/test_trash_operations.py`, `test_trash_client.py` and `test_trash_state_qml.py`. These create temporary roots, use inert bus addresses and never inspect or change the user's Trash. Core acceptance covers changed identities/records/descendants, symlink targets, late arrivals, limits, durability failures, rollback collision, stop, interrupted journal and native filesystem deletion. The actual private UNIX service/client proof closes/restarts clients and service, proving duplicate confirmation does not delete a later replacement. The Qt fixture exercises the production confirmation/state components with a recording transport; native pointer/keyboard evidence is recorded separately when run.
