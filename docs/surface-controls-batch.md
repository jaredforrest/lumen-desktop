# Surface and control repair batch

This checklist separates delivered source changes from live visual acceptance.
One combined build and visual pass covers the batch; a source check alone is not visual acceptance.

| Area | Expected result | Implementation | Live acceptance |
|---|---|---|---|
| Dock | Transparent padding above the shelf has no dark slab | Installed | Prior focused check passed; include in combined pass |
| Search and panels | One rounded material surface, without a second shadow halo | Installed | Combined pass pending |
| Notification centre | Individual cards own their material; no enclosing duplicate border | Installed | Combined pass pending |
| Window controls | Same 20-pixel targets and 12-pixel circles | Installed | Combined pass pending |
| Inactive controls | Hover reveals glyphs without recolouring inactive circles | Included in batch | Combined pass pending |
| Alternate green click | Fill the window, matching native controls | Included in batch | Behaviour check pending |
| Green hover menu | Clicking the covered original control still performs fullscreen; alternate click fills | Included in batch | Behaviour check pending |
| Close and minimize | Affect the selected window; close, minimize and restore are checked on disposable windows | Existing implementation | Combined pass pending |
| Corner rendering | Inspect actual installed surfaces at native scale for remaining jagged edges | Shared material repair installed | Combined pass pending; no unverified compositor claim |

The delayed native green-button panel is a layout menu, not a tooltip.
Desktop-owned integrated controls do not yet open that menu on hover. This remains an explicit consistency gap rather than a completed feature.
