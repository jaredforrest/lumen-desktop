# Surface and control repair batch

This checklist separates delivered source changes from live visual acceptance.
One combined build and visual pass covers the batch; a source check alone is not visual acceptance.

| Area | Expected result | Implementation | Live acceptance |
|---|---|---|---|
| Dock | Transparent padding above the shelf has no dark slab | Installed | Combined installed screenshot passed |
| Search and panels | One rounded material surface, without a second shadow halo | Installed | Installed search screenshot passed; other panel variants remain to sample |
| Notification centre | Individual cards own their material; no enclosing duplicate border | Installed | Combined installed screenshot passed |
| Window controls | Same 20-pixel targets and 12-pixel circles | Installed | Native controls inspected at original scale; owned-control interaction remains to sample |
| Inactive controls | Hover reveals glyphs without recolouring inactive circles | Included in batch | Combined pass pending |
| Alternate green click | Fill the window, matching native controls | Included in batch | Behaviour check pending |
| Green hover menu | Clicking the covered original control still performs fullscreen; alternate click fills | Installed | Physical green click after hover passed; alternate click remains to check |
| Close and minimize | Affect the selected window; close, minimize and restore are checked on disposable windows | Existing implementation | Physical close and minimize plus restoration passed on a disposable window |
| Corner rendering | Inspect actual installed surfaces at native scale for remaining jagged edges | Shared material repair installed | Native control corner crop inspected at original scale; no claim of exhaustive corner acceptance |

The delayed native green-button panel is a layout menu, not a tooltip.
Desktop-owned integrated controls do not yet open that menu on hover. This remains an explicit consistency gap rather than a completed feature.

## Batch receipt

Installed implementation: `074b4135da807e2fe86e887ee2eaecfd09de50b7`.
The desktop package and source-contract checks passed before activation.
Installed Dock, search and notification screenshots were reviewed together.
The disposable window and temporary input device were removed after the checks.
Screenshot evidence stays local because it includes personal desktop content.
