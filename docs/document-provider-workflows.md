# Document print and version capabilities

These are separate provider capabilities. PDF export is not printer submission,
and an in-memory snapshot is not persistent document history. The retained
providers are version 0.5.0; the spreadsheet uses the separately pinned native
close patch. `document_provider_workflows.py` records exact installed executable
hashes and runs in the shared disposable filesystem/network/process boundary.
It never connects to a host printer service.

## Confirmed command-line observations

The word processor can capture a labelled session snapshot, restore its content,
and refuse an invalid snapshot index without writing a misleading output file.
The saved document reopens with an empty snapshot list. These snapshots therefore
provide session-local recovery only; this is not persistent version browsing.
The source document remains unchanged when a restored copy is written.
Snapshots belong to the current logical document: New and a successful Open
clear the previous document's snapshots. A failed Open preserves the current
document and its snapshots. Restoring within the same document keeps its
snapshot list available for repeated restores. The installed
`word_version_ownership.py --writer /nix/store/.../bin/wordcraft-cli --output ...`
fixture separately checks these boundaries inside the disposable namespace.
This ownership fix does not add persistent history or a version-browser UI.

The spreadsheet's `file.print` action can produce a PDF at a chosen file path.
The test independently parses the PDF's pages and text, checks the source hash,
and verifies an unavailable destination returns failure without producing a file.
This proves a print payload, not an interactive destination/options dialog or a
printer queue.

## Native workflow observations

The word processor's Print page offers a Print action and PDF export. Its pinned
Linux backend generates a private temporary PDF and opens the configured PDF
viewer. A test-only private association selects the retained browser, delays its
startup, and verifies the exact native process, PDF argument, surviving PDF hash
and actual painted document page. It does not substitute a fake viewer.

The retained viewer's real Print preview exposes destination, page and layout
controls. Physical Escape, cancellation of its Save chooser, an unavailable
folder, and explicit Save as PDF are exercised separately. PDF text is parsed
independently and source hashes remain unchanged. The word processor's distinct
PDF-export chooser is also tested for Cancel, unavailable folder and Save.
These results establish print-to-file and cancellation through the configured
viewer. They do not establish physical printing or acceptance by a printer queue.

Without a configured viewer, the provider reports a precise handoff failure but
its Print page originally hid the status bar. The narrow packaged source patch
returns to the document on failure so the existing status message is immediately
visible. Successful handoffs preserve the Print page. The source build retains
the release's exact embedded font input and font licences.

The spreadsheet and presentation packages contain separate provider-owned
Print to PDF actions. Each uses the provider's existing PDF renderer and a real
PDF-only destination picker. They do not provide physical printer queues.
The spreadsheet's advertised shortcut is wired to the same command as its menu.
The presentation reuses its PDF layout choices while preserving ordinary export
formats. Its captured document identity, revision, path and selected slide are
revalidated before output, including after an asynchronous picker returns.
A changed document retires the captured dialog rather than redirecting its action.
The PDF subset renders bytes before publishing them through exclusively created
staging in an opened destination directory, then atomically replaces the chosen
entry. It never writes through a destination or predictable temporary-file link;
a retargeted selected parent is rejected and owned staging is removed.
Native acceptance for these actions is recorded separately with `--pdf-output`;
source implementation alone does not establish working picker interactions.
No generic printing or version-history service compensates for missing provider
workflows.

## Reproduction

Run `tests/desktop/document_provider_workflows.py` through
`tests/ci/run-native.sh`, using `desktop-test-python` and absolute pinned
`--writer`, `--sheet`, `--slides` CLI paths plus `--pdf-tools`. The outer runner
creates an owned empty artifact directory and enters the disposable boundary.
It records every command, exit status and output digest. Five CLI checks cover only the command observations. The existing
`office_native_workflows.py --document-probe` exercises the native workflow;
`--pdf-viewer`, `--xdg-tools`, `--mime-data`, `--at-spi-prefix`, `--dialog-tools`
and `--pdf-tools` must name immutable selected dependencies. The browser gets a
fresh private profile and private PDF association. Ten native checks cover the
configured viewer route; a separate unavailable-viewer run checks immediate
failure visibility. Persistent version browsing remains unsupported by the
observed session snapshots. These optional installed-provider fixtures are
separate from the ordinary shell CI lane.

For the spreadsheet and presentation subset, use `office_native_workflows.py`
with `--app sheet` or `--app slides`, `--pdf-output`, `--dialog-tools`,
`--pdf-tools` and the same pinned physical keyboard/pointer dependencies.
The test uses real Cancel, unavailable destination and Save interactions, checks
source hashes and parses generated PDF content independently. The presentation
also holds the Save control while the document is changed or replaced, then
releases it without moving the pointer and checks that no bystander was mutated.
All capability records remain incomplete for physical printer submission.


## Native save publication

All three packaged Linux providers share the same publication helper for native
saves and copies. It reserves a new private staging file with exclusive creation,
opens the selected parent as a directory, writes and synchronizes the bytes, then
checks the staging file identity and selected directory identity before renaming.
Existing regular-file permissions are preserved; new files start at mode 0600.
An existing destination link is replaced rather than followed. Old predictable
staging names are neither opened nor removed. A refused publication removes only
its own unchanged staging file.

`provider_atomic_save.py` runs the installed command-line binaries in detached
user, process, network, mount, IPC and hostname namespaces. Its 21 cases cover
existing staging hardlinks/symlinks, destination hardlinks/symlinks, a FIFO parent,
publication failure cleanup and ordinary saves for each provider. Original hashes,
output container contents and relevant permissions are checked independently.
The package checks also cover directory retargeting and staging replacement using
a private test callback. The callback is absent from the provider API.

These checks establish deterministic alias and failure handling. They do not
establish protection from a hostile process running as the same user, directory
synchronization across power loss, persistent version history, or format fidelity.
Asynchronous chooser ownership is a separate UI contract: a safe file publication
cannot itself decide whether a delayed chooser still belongs to the open document.


## Native chooser ownership

The word processor captures the requesting document identity, revision, dirty state, protected encryption state and saved
path before starting its native chooser. Before accepting an Open, Save As or
Export result, it rechecks that capture. Picture requests also capture the
selection and pending replacement target; recipient-list requests capture their
existing merge state. A late result refuses with an actionable message before
any write, document replacement or recipient/picture mutation. Refused results
and failed saves return from Backstage so the existing status message is visible.
Canceling a stale
request does not clear a newer operation's state. Ordinary same-owner Save As
and cancellation retain their existing behavior.

`office_native_workflows.py --app writer --picker-ownership --dialog-tools ...`
uses the actual installed provider, native chooser and physical keyboard in the
disposable namespace. Supported provider commands replace or edit the document
while the chooser is open. The test waits for the provider's read-only
`fileDialogOpen` acknowledgement before assessing the result; it checks complete
content, saved path, dirty state, original-file hashes and independently parsed
successful output. Source-level asynchronous tests separately cover Export,
picture selection/target, recipient replacement and stale Save Changes
continuations. This does not add cancellation to a foreign host's legacy
`open_async` callback interface; the selected native host uses the owned
`Services::file_dialog` response channel.

## Document copy export and conversion decisions

The word processor uses a separate `file.exportCopy` command for Export. It
serializes current unsaved content without stamping Save metadata, changing the
active path, clearing dirty state or replacing the working document. PNG export
uses the same descriptor-anchored atomic publication as other provider outputs.
The selected filename must match the declared export suffix, case-insensitively;
changing an export filename to another format is refused before publication.

Known structural-loss formats (including PDF and PNG) require an explicit
Continue or Cancel decision before the export chooser opens. A protected
working document also requires that decision for every output that cannot
preserve its password, including native JSON. The warning separately discloses
that the converted file will be unencrypted. Save As bases the decision on the
actual selected path after the chooser returns. Cancel creates no output.

The decision captures the document owner, revision, dirty state, saved path and
protected state. Replacement, edits or a detached/replaced dialog retire that
request. A stale or repeated request cannot act on a newer operation. For
filesystem Save As, a retained parent descriptor and selected entry identity
are checked again before staging and before publication. A newly created,
replaced or edited destination is refused without changing that destination or
marking the working document saved. This is bounded cooperative concurrency;
it is not an atomic compare-and-swap, hostile same-user protection or a
power-loss guarantee. Normal Save retains its existing behavior.

`office_native_workflows.py --app writer --copy-export-probe
--copy-export-format docx|odt --dialog-tools ... --at-spi-prefix ...` exercises
the installed provider with physical pointer/keyboard controls, its actual
accessible button semantics and native destination chooser. It records full
working-document projections, original hashes and independently parsed output.
The DOCX scenario checks copy ownership and mismatched suffix refusal. The ODT
scenario checks Cancel, held Continue after an edit, valid copy output, Save As
Cancel, a destination created during the decision and valid Save As continuation.
Both scenarios run in a disposable namespace with frozen source/tool identities.
Source tests separately cover all supported suffix aliases, protected JSON,
PNG aliases, one-shot decisions, failed output and download-host behavior.
These tests do not establish complete conversion fidelity or encrypted format
interoperability with another application's implementation.

## Presentation output formats

The presentation provider checks the requested output family before writing or
marking the document saved. Unsupported suffixes, unknown explicit formats and
conflicting known suffix/format pairs fail without replacing an existing
destination. The in-memory download path also propagates an unsupported format
error instead of presenting an empty or native-container download as success.

The existing native `.deckcraft`/`.slidecraft` and extensionless paths remain
available, as do case-normalized presentation, PDF, outline and image exports.
The existing `.potx` and `.ppsx` aliases still contain ordinary presentation
content; they are not evidence of template or slide-show format semantics.
There is no ODP exporter. Refusing ODP is preferable to reporting success for
native-container bytes bearing an ODP suffix.

`deck_format_truthfulness.py` runs the installed command-line provider in the
disposable application namespace. It checks refusal against existing destination
sentinels and original source hashes, verifies supported container signatures,
and checks that an unsupported conversion produces no output. Source tests
additionally retain the complete document, identity, revision, path, dirty state
and selection across refusal, and exercise legitimate outline and image output.
These checks establish format honesty and source preservation, not complete
cross-format fidelity or a new format engine.
