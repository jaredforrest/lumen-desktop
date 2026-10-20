# Retained office workflow acceptance

`tests/desktop/office_workflows.py` runs the installed headless providers inside
`application_fixture.isolation`. The disposable mount, process and network
namespaces retain the invoking non-root identity, remove capabilities, expose
only immutable store/source files and a fresh owned artifact directory, and use
private profiles. No GUI, account, live document, printer or external server is
used by this phase.

Pass the exact `wordcraft-cli`, `gridcraft-cli` and `deckcraft-cli` executables and
the pinned `.#test-office-tools` output with `--writer`, `--sheet`, `--slides`,
`--pdf-tools` and a new `--output` directory. The development shell supplies the
pinned Python runtime. This suite is separate from ordinary backend unit tests;
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

The historical CLI report's `capabilities` array contains scoped action observations. It is not a complete per-capability conformance record. Generate a separate record set with `tests/desktop/office_capability_records.py --cli REPORT --gui NATIVE_REPORT --output NEW_FILE` (repeat `--gui` as needed). Each provider/capability record includes the normative identity, permission, offline, scenario, difference and status fields. A partial successful fixture remains `implemented_unverified`; unassessed actions remain explicit, and no record is promoted to full compatibility or reference verification. The generator hashes evidence and never rewrites the input report.

The installed-window prototype uses the shared persistent full-US physical keyboard. Per-command virtual keymaps are unsuitable for these providers' physical-key handling. Text is paced at 40 ms per character, then read-only live provider acknowledgement is required before Save. This is a fixture delivery policy, not a reference latency measurement. The prototype currently proves physical edit/save for three providers and Writer dirty-close Escape/pointer Cancel in its fixed private viewport; other close and recovery paths remain outstanding. It does not measure desktop appearance parity.
