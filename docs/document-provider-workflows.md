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

The spreadsheet engine returns a PDF for `file.print`; its UI only routes
`file.exportPdf` to the download hook. The presentation provider has PDF export,
but no print command was found in its pinned command/UI source. Physical Print
invocation and Escape retain the captured provider UI and source hashes; these
observations are not described as working print destination dialogs. No generic
printing or version-history service compensates for missing provider workflows.

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
