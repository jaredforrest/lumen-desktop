# Symbolic-link substitute

Files deliberately names this action **Make Symbolic Link**. It creates a new
link without replacing an existing item. A link remains distinct from a copy in
the file views, accessibility labels and File Info. Opening a resolved link
follows its target; metadata/tag and link-removal actions operate on the link.
The substitute does not track a target that has moved.

Listing reads the link's own inode facts first. A missing, permission-denied or
otherwise unavailable target does not remove that row from the listing. A link
chain rejected by the operating system remains visible as an unresolved link.
The kernel's chain-limit error does not distinguish an actual cycle from a long
acyclic chain, so the UI does not claim that it can tell them apart.

Opening an unavailable link reports the cause where known and directs the user
to File Info. File Info keeps the literal target and a selectable status message;
it does not silently change the reference, search personal folders, launch a
replacement, or remove the original target. Normal directory-link navigation
remains supported. External application handling is a pathname handoff, not an
end-to-end descriptor or target-identity guarantee.

Acceptance uses disposable real files for unique creation, structured dispatch,
target moves, cyclic links, directory links and target errors. External launch
is observed rather than executed. The Qt label test
loads the production column and information components on a private offscreen
session. These checks do not certify external handlers or visual reference
equivalence.
