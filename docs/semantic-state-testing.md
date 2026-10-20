# Semantic colours and non-hue states

The existing semantic-palette unit matrix checks foreground, secondary, error,
success and warning contrast on each semantic surface, selected text and focus
roles across every supported accent, both appearances and increased contrast.
Existing actual QML selection tests cover preview rows and filled calendar days;
material hierarchy, calendar visibility marks and native focus corpora retain
separate scopes. This batch does not replace those tests or classify every
literal colour as a defect.

The literal-colour inventory distinguishes these purposes:

| Purpose | Existing examples | Treatment |
| --- | --- | --- |
| Semantic roles and fallback palette | Theme foreground/status/selection roles | Resolved contrast matrix; essential states also need visible meaning |
| Choice or optical artwork | Appearance previews, named accent/tag swatches, calendar icon, screen-saver artwork | Retain intentional values; they depict the chosen content or a fixed canvas |
| Affordance artwork | Window-control circles, switch thumb, scrollbar thumb | Retain optical fills; verify identifying boundaries as well as shape, position and state |
| Compositing treatment | Modal dimming, popover shadows, Dock/menu materials | Existing live transparency and material tests; not foreground semantics |
| Wallpaper labels | Desktop file white outlined labels | Wallpaper-dependent presentation, not a claim of arbitrary-background contrast |
| Provider decoration | Provider panel brand colours | Does not authorise changing provider behaviour or configuration |

A reproduced StatusMessage gap was narrower: the severity prefix existed only in
the accessible name. With the same message, error, warning and success changed
visible hue without another visible severity cue. The fixed component paints a
short severity prefix while preserving callers' raw `text`, semantic colours and
existing accessible name/announcement. The prefix is ignored as a separate
accessibility child to avoid duplicating the message.

`semantic_state_paint.py` captures genuine production Qt frames in a complete
private namespace. Eight mode/contrast/width combinations cover message wrapping,
painted semantic foreground contrast, switch position, connection subtitle and
disabled action context. It retains grayscale, protanopia and deuteranopia
transforms. These are diagnostic software transforms, not human usability or
clinical vision validation. The non-hue assertion checks visible severity text,
not a simulated-colour difference alone.

`semantic_state_native.py` uses the same namespace boundary with a real private
compositor and physical keyboard protocol client. Across four mode/contrast
combinations it toggles the actual switch and connection control, verifies the
positional/textual state, refuses an unavailable control, preserves the disabled
action and checks the unchanged native window. No external application, account,
radio, setting or file action is invoked. These are local owned-control proofs,
not reference-exact colour, third-party or hardware acceptance.

The switch indicator also had a reproduced boundary gap: 26 of 64 actual
resolved mode/accent/contrast/on-off frames had a white indicator boundary below
3:1 against the track. The white optical thumb is retained; a one-pixel outline
uses the existing contrasting foreground selector for the actual track fill.
`switch_indicator_paint.py` verifies the boundary is genuinely painted before
counting its contrast. Rectangle border getters have defaults even when no
border is drawn, so metadata alone cannot establish that evidence. All 64
combinations retain their original dimensions and on/off positions. This is an
enabled-indicator boundary check; disabled opacity remains a separate policy.

The native unavailable transition exposed a Qt warning when ControlTile tried to
remove its tab-focus capability while it held focus. Unavailable tiles now use
the standard disabled-item policy, retaining their visible labels and accessible
unavailable explanation while Qt retires focus and skips them during traversal.
The native regression observes focus leaving, Tab reaching the next enabled
control, and Return refusing action. A persistent private pointer physically
clicks the disabled tile; a background observer receives the otherwise unhandled
click at the captured client coordinates, proving input delivery without tile
action. The log gate also runs after all children are reaped and rejects every
Qt warning/error rather than suppressing the original diagnostic. ControlTile
has no existing dedicated hover tooltip to preserve; visible and accessible
unavailability remain discoverable.
