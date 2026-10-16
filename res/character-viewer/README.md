# Emoji & Symbols

A native Wayland GTK3 character palette for the Sway desktop. Open with
**Ctrl + Super + Space** (Super is the PC Windows key, acting as Command).
**Super + period** and the existing **Emoji and Symbols** launcher also open it.

- Compact 352 × 402 panel, Lumen emoji artwork, category strip and search.
- Type to search; Down enters the grid, arrows navigate, Return inserts,
  Escape dismisses. Clicking outside dismisses it.
- A session service uses Wayland input-method-v2 and a GTK custom surface to
  anchor the panel beside the caret when the app supplies a cursor rectangle.
  It keeps application focus and commits the emoji directly, preserving the clipboard.
- If an app lacks text-input support or a cursor rectangle, the ordinary floating
  picker is used automatically. Click an emoji to return focus and insert it. Brave,
  Chromium, Google Chrome and Codex/ChatGPT use clipboard paste; the selected character remains
  on the clipboard. Other apps use `wtype` without changing the clipboard. If
  that window has gone or typing reports failure, copy the character and show a
  paste notification. Apps that silently ignore virtual typing can use explicit Copy.
- Right-click or hold a character for skin tones, favorites and explicit Copy.
  Skin-tone preferences, the last 40 choices and favorites survive restarts.
- The compact home view combines up to two natural rows of recent choices with
  the main emoji category. The expanded view shows all saved recent choices.
- The top-right button opens a three-column viewer with a scrolling category
  list, character grid, large preview, Unicode code, favourites, Insert and Copy.
  Single-click selects; double-click or Return inserts. The chosen view persists.
- Expanded character categories include arrows, currency, accented Latin,
  letterlike symbols, maths and punctuation. Search accepts names and Unicode
  codes such as `U+1F600` or `00A3`.
- Ctrl+F focuses search; arrows traverse the grid; Menu or Shift+F10 opens
  character options. Escape closes options before dismissing the picker.
- Skin-tone options use the shared palette, wrap and scroll within a bounded
  popover, and include mixed-tone handshakes.
- State: `$XDG_STATE_HOME/character-viewer/state.json` (normally
  `~/.local/state/character-viewer/state.json`). No network access at runtime.

## Scope and differences from desktop

This is a local approximation of Character Viewer, not Lumen's application.
Caret anchoring is available through Sway's input-method protocol rather than
ordinary window IPC. Apps must publish their cursor rectangle. The tested Brave
1.85.118 configuration enabled text-input without publishing a rectangle, so it
uses the floating/clipboard fallback. An active input-method service such as
Fcitx owns the same one-per-seat role; this picker leaves it in place and falls
back if the compositor reports that role unavailable.

The connected generic UK USB keyboard (04d9:4545) does not advertise KEY_FN,
KEY_KBD_LAYOUT_NEXT or KEY_EMOJI_PICKER. No existing key was repurposed.
Ctrl+Super+Space and Super+period remain available. XF86EmojiPicker is also
bound for keyboards that emit a dedicated emoji key event.
There is no Genmoji or automatic replacement of the word before the caret.
The expanded view closes after insertion rather than remaining as a separate utility window. Application support for
Wayland virtual-keyboard Unicode input varies; Copy remains available.

Unicode Emoji 15.0 is bundled to match the configured Lumen 16.4 artwork.
Newer Unicode emoji are deliberately excluded because unsupported joined
sequences otherwise display as several separate pictures.

## Sources

- Data: https://unicode.org/Public/emoji/15.0/emoji-test.txt
  (see `UNICODE-LICENSE.txt`).
- Font: the existing `home/lumen-emoji.nix` pinned package, registered explicitly in the picker. The desktop fontconfig configuration also
  prefers Noto Color Emoji with fallback for missing glyphs.
- Category icons: the existing Lumen icon-theme checkout, files from
  `src/categories/symbolic`; https://github.com/vinceliuice/Lumen-icon-theme
  (see `icons/COPYING`).

## Validation, 2026-10-02

Nix package build and Python compilation passed. Live GTK checks covered
compact dimensions, search, six thumb skin-tone variants, persisted favorites,
empty search, non-emoji symbols, and expand/collapse. A separate GTK text field
received the exact Unicode character 👍 after search + Down + Return. The same insertion test passed through actual Ctrl + Super + Space key events. Escape dismissal
and native Wayland app identity were checked. The section-E shortcut watcher
restores the bindings after Sway reload and is shared with the existing
screenshot/recording conveniences.

Screenshot: `artifacts/visual-tests/character-viewer.png`.

Final integration: exact U+1F44D insertion into a live Brave textarea passed
after restarting the shortcut service and reloading Sway. Explicit focus after
positioning prevents focus-follows-mouse from dismissing the panel on startup.

## Caret service and diagnostics

`lumen-character-input.service` starts with `sway-session.target`. It listens on
an owner-only socket inside XDG_RUNTIME_DIR, scoped to WAYLAND_DISPLAY. The
normal launcher asks this service to show/dismiss the picker. If the service is
absent, the launcher still opens the floating picker. `--standalone` explicitly
uses that fallback; `--status` reports service/placement state without text data.
Surrounding text sent by applications is discarded, never logged or saved.

The GTK popup protocol role is released before its underlying wl_surface. Keys
are grabbed only while the caret popup is open; opening, dismissal, field changes
and commit restore ordinary application input. A field-generation check prevents
committing a selection to a different field after a focus change.

Caret integration validation passed in an isolated Sway 1.11 session: repeated
open/commit/dismiss, exact Unicode insertion, unchanged clipboard for native
commits, skin-tone insertion, field-switch dismissal, missing-rectangle fallback,
and actual Ctrl+Super+Space / XF86EmojiPicker events. The live user service and
shared shortcut watcher are active; logout/login was not exercised.

Evidence and scoped rollback links:
`artifacts/docs/section-e/evidence/caret/`.


## Stage 4 refinement (6 October 2026)

The expanded viewer shares the existing caret/floating architecture, rather than
creating a separate movable utility window. In narrow native target windows its
preview column can be omitted to keep the picker usable. Minimum compact width
is 352 logical pixels; extremely small outputs remain a limitation.

Unicode emoji data remains pinned to 15.0 to match the installed artwork. The
extra symbol categories are selected named Unicode blocks, not every Unicode
script or Lumen's complete character catalogue. Genmoji, text replacement,
font-variant browsing and universal app font enforcement are not implemented.
Applications that draw their own emoji images or ship a hard-coded webfont can
still look different. A system font preference cannot replace those images.

Fallback virtual typing now allows the receiver time to process the virtual
keyboard's keymap before sending a joined sequence. Chromium and Codex/ChatGPT paste still leaves
the inserted character on the clipboard. Native commits preserve it.

Reproducible isolated tests and screenshots:
`artifacts/stage4/` (test.py, harness.py, editor.py).
The tests use disposable profiles/state and do not type into signed-in apps.
