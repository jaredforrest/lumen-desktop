# Retained office workflow acceptance

`lumen-office-workflows` (Rust 2024) runs the installed headless providers inside
the packaged application isolation boundary. The disposable mount, process and network
namespaces retain the invoking non-root identity, remove capabilities, expose
only immutable store/source files and a fresh owned artifact directory, and use
private profiles. No GUI, account, live document, printer or external server is
used by this phase.

Pass the exact `wordcraft-cli`, `gridcraft-cli` and `deckcraft-cli` executables and
the pinned `.#test-office-tools` output with `--writer`, `--sheet`, `--slides`,
`--pdf-tools` and a new `--output` directory. Build
`.#test-office-workflows-fixture` and invoke its binary with `--source .` and
those exact provider paths. This suite is separate from ordinary backend unit tests;
optional office applications must be built explicitly.

The corpus contains styled document paragraphs, two sections, a table, a generated
image, a link, header/footer text, a comment and tracked insertions; workbook
formulas, references, number/date formats, sorting/filtering metadata, a chart,
multiple sheets and a print range; and presentation layouts, text, an image,
a shape, notes and a transition. Unsupported formulas must produce an explicit
error value. Intermediate edited copies establish that edits occurred before
undo and same-format save/reopen.

Independent ZIP/XML readers compare content, formatting, references, sections,
review data, media, geometry, layouts and fonts. PDF tools independently inspect
page counts and text and rasterize both exports. Painted differences and every
structured difference are retained. SHA-256 checks ensure export/conversion never
mutates the captured source. Container identity is checked independently of file
extension and provider exit status. This is a small reproducible corpus, not a
claim that all possible documents or font/layout combinations retain fidelity.

The result separates `harnessPassed` from `workflowComplete`. A completed test
harness does not close the office specification. Each capability is recorded as
fixture-verified, degraded, unsupported, a provider defect or not yet verified.
GUI open/edit/save, unsaved-close choices, recovery, templates/recent entries,
format-loss warning cancellation, presenter view, print-dialog interaction,
collaboration/version history and interactive PDF features require separate
acceptance. A print dialog or generated PDF does not establish physical printing.

The installed 0.5.0 providers currently have important format limits: the writer's
DOCX-to-ODT-to-DOCX probe loses review and layout information; spreadsheet ODS
export is explicitly unsupported; presentation CLI conversion to an `.odp` name
can return success while writing its own native container. The latter is recorded
as a provider defect, never renamed or counted as a standard-format export.
Native OOXML and PDF checks remain separate from those limitations. ODT
same-format checks start from the explicitly measured converted fixture and do
not erase its earlier losses. No application replacement or proprietary reference
implementation is implied by these tests.

The historical CLI report's `capabilities` array contains scoped action observations. It is not a complete per-capability conformance record. Generate a separate record set with `nix develop --command lumen-office-capability-records --cli REPORT --gui NATIVE_REPORT --output NEW_FILE` (repeat `--gui` as needed). Each provider/capability record includes the normative identity, permission, offline, scenario, difference and status fields. A partial successful fixture remains `implemented_unverified`; unassessed actions remain explicit, and no record is promoted to full compatibility or reference verification. The generator hashes evidence and never rewrites the input report.

The Rust 2024 installed-window fixture `office-native-workflows` uses the shared persistent full-US physical keyboard. Per-command virtual keymaps are unsuitable for these providers' physical-key handling. Text is paced at 40 ms per character, then read-only live provider acknowledgement is required before Save. This is a fixture delivery policy, not a reference latency measurement. The fixture proves physical edit/save and reopen for all three providers, new Save As through the private chooser, and the supported dirty-close choices including spreadsheet save failure. Recovery remains outstanding. It does not measure desktop appearance parity.

Reopen acceptance creates a new empty document in the actual installed window, confirms the previous saved path is detached, then opens that exact saved file through the provider's native command dispatcher. It compares Writer full text and document-inspector blocks/runs/styles/tables/sections/parts; every bounded synthetic spreadsheet cell's native details plus computed/formula/formatted ranges and sheet metadata; and every slide inspector's text/geometry/style. Opaque `id` fields are not stable document identity. Writer selection and undo/redo availability are transient session state and are excluded; other document fields remain compared. Reopen must preserve the independently parsed file and its hash. Retained painted output supports inspection but does not establish reference geometry or full visual fidelity by itself.

Use `--effects` only with the pinned effects compositor. It explicitly opts into the shared isolation helper's validated software allocator device; normal mode still exposes no graphics device. The compositor must report GLES, llvmpipe, initialized effects shaders, the software allocator and created framebuffers. Both modes require a nonblank captured application paint. The Mesa vendor/Vulkan files come from the explicit immutable software-driver argument, never from live desktop preferences. These checks verify the renderer actually exercised; they do not turn a software-rendered screenshot into reference fidelity evidence.

The spreadsheet provider is built from its exact pinned 0.5.0 source and Cargo
lock with a narrow native-close patch. Native close commits pending drafts,
captures the original document identity, and uses the provider's existing
Save/Discard/Cancel flow. It does not force termination. Multiple dirty documents
require separate decisions; cancellation, failed saves and unrelated dialogs keep
the application open. Five Rust library tests run with compiler warnings treated
as errors. The release assets and installed profile wrapper remain in use.

The common installed wrapper provides the DBus library required by the native
chooser loader. To exercise new-file saving, pass `--new-saveas --dialog-tools
STORE_FILE`, where `STORE_FILE` is the `.#test-office-dialog-tools` output. The
fixture launches only its pinned standard portal and GTK chooser backend on an
explicit activation-free private bus. It provides public fonts/icons, preserves
an inert system bus, and never uses host portals, document mounts or services.
The standard frontend can report unavailable unrelated document, realtime,
location or camera services; retain these diagnostics rather than hiding them.
The picker currently supplies no parent-window association, which is retained as
a provider limitation, not inferred from its appearance.

The new-document scenario uses actual physical input and the real chooser. It
checks cancellation, an invalid destination under the read-only source mount,
complete draft retention, a later exact successful destination, independent
native-format parsing and a subsequent clean native close. Spreadsheet native
Close-to-Save enters this real chooser. The separate `--sheet-close-outcome`
scenarios cover Cancel, Discard, Save and a real existing-file save failure;
`--slides-close-outcome` covers its three decisions. Saved-file hashes and full
unsaved markers remain checked. A successful Quit uses the retained pre-close
application paint; an empty desktop after a legitimate close is not an app paint
failure. Recovery, additional provider save-error paths and the other outstanding
workflow clauses remain separately unverified.
