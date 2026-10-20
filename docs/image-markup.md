# Image markup copies

Quick Look exposes **Annotate a Copy…** for still PNG, JPEG and WebP images.
The owned preview provider first validates the retained image's codec and dimensions;
the editor's image source remains empty until that captured capability acknowledges
support. Drawing, rectangles, three ink colours and Undo operate on an in-memory
recipe. Add Rectangle also provides a keyboard-reachable annotation without a
pointer gesture. Before confirming Save Copy, closing the editor or cancelling its chooser writes
no file and leaves the original untouched.

Save Copy explicitly creates a new PNG. An existing filename, the original path or
an existing symlink is refused; this workflow has no overwrite mode. The chooser
captures the provider session, source capability, editor generation and recipe.
Each chooser instance owns that capture, so a late result cannot target a new
preview. A failed save preserves edits and explains the failure. Successful export
flattens raster annotations in the original pixel order, consistently with the
explicitly disabled auto-transform image preview. EXIF rotation is not applied: the
pinned Qt image codecs do not consistently honour it across JPEG and WebP. Other source metadata, colour-profile fidelity, animation,
PDF editing, signatures and foreign-application annotation APIs are not claimed.

The decoder accepts up to 32 million pixels. Recipes accept 512 strokes and 8192
normalised points, with pen width between 0.001 and 0.05 of the shorter image edge.
Before drawing, a conservative 64 million raster-work-unit budget accounts for
pixel distance times pen width and wide joins. Excessive work is refused before
constructing a drawing context. Pen rendering checks cancellation between bounded
segments. These are implementation safety bounds, not measured reference limits.

The existing preview broker validates original file facts and pins the immutable
provider resource throughout metadata inspection and export. The image is opened
through a regular-file-checked retained descriptor. Publication uses a private
staging file in a pinned destination directory, then an exclusive link. The
original requested parent is re-resolved before publication; an alias retarget or
replacement fails closed. Existing destinations cannot be truncated or replaced.
Unpublished staging files are removed on failure/cancellation. A committed copy
with a directory-durability acknowledgement failure is reported as saved with a
warning. Export has no interactive cancel-after-confirmation promise; preview
teardown requests cooperative cancellation and waits for the owned action worker.

`test_image_markup.py` exercises actual disposable filesystem copies, original
bytes, collisions, source replacement/retirement, failure cleanup, denied/missing
folders, unsupported special files, header pixel limits, raster work and parent
alias retargeting. `test_image_markup_qml.py` runs the production editor in actual
Qt and checks captured requests, cancellation, stale acknowledgements and resource
source clearing. `image_markup_input.py` uses an installed Quick Look, actual
pointer controls, a private accessibility bus for read-only chooser field bounds,
and the shared acknowledged physical keyboard. It never sets text or dispatches
an accessibility action. Fixtures use synthetic images and private HOME/XDG/bus
state; no account, live clipboard or host document is used.
