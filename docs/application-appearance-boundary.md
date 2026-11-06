# Application appearance boundary

The desktop owns compositor frames, shared shell surfaces, and authentication
surfaces. Applications own their document content, tabs, toolbars and client-side
headers. A supported application setting may select native framing or system
appearance; the compositor must not crop client content to imitate that setting.

## Frame ownership

| Surface | Frame authority | Required result |
| --- | --- | --- |
| Shell application with an integrated toolbar | Shell client | No additional compositor title bar |
| Application requesting native decoration | Compositor | One outer frame; application content remains intact |
| Portal or toolkit file chooser negotiating client decoration | Toolkit client | Preserve its negotiated header; no second title bar |
| Browser download bubble, extension popup or transient menu | Application | Keep its transient surface distinct from a file chooser |
| Lock and login | Authentication client | Shared visual component; unlocking still requires the real authentication backend |

The Save/Open chooser's rounded content or internal toolbar does not justify
adding or removing an unrelated outer frame. This contract follows negotiated
surface ownership; it does not promise that every toolkit exposes identical
header content or corner geometry.

## Supported appearance adapters

Brave receives system-theme and declared native-frame settings while stopped.
Obsidian receives its supported native-frame preference while stopped. GIMP
receives its supported system colour-scheme preference while stopped. Inspection
commands such as `--help` and `--version` never create or rewrite these profiles.
Adapters preserve unrelated document and application state. Unknown application
interfaces, executable content cropping, and proprietary service emulation are
not styling mechanisms.

GTK uses the declared theme, font, left-side button layout and desktop colour
scheme. Qt receives the existing platform-theme bridge. Application-specific
interiors still require supported integration and separate verification; a shell
or frame test cannot establish document-layout, vault or office compatibility.
Do not overwrite document fonts merely to change interface typography.

## Authentication presentation

Login and lock share `AuthView.qml`. Identity and backend error strings render as
plain text, the response field has an accessible name, actionable controls expose
keyboard focus, and clock/name layout is bounded by the surface. Password reveal
is explicit and its state is cleared on submit, Escape, visibility changes,
authentication transitions and power requests. These presentation rules do not
change PAM, greetd, screen-lock ownership or permission decisions.

The isolated authentication fixture checks shared layout, response clearing,
reveal behaviour and presentation semantics. Native-window preference process
tests check retained state and non-mutating inspection commands. Real application
and portal acceptance must additionally inspect their rendered window trees;
this document is an ownership rule, not evidence that every retained interior
has been visually matched.


## Consumer file artwork

`LUMEN_FILE_ICON_OVERRIDES` accepts a JSON object mapping an extension (for example `.csv`), a MIME type, or a category (`folder`, `image`, `table`, `audio`, `archive`, `source`, `document`) to an image URL. Extensions take precedence over MIME types, then categories. Consumers supply and own these assets; none are bundled by this interface. Missing or unreadable overrides fall back to the built-in artwork. An empty or malformed object leaves defaults intact. Supply the environment value through the consumer session configuration.

The character viewer similarly accepts a consumer font file and family. Its
process-local Fontconfig rule and refreshed Pango font map keep emoji requests on
that configured family; they do not alter system font settings or ordinary text
families. Consumers remain responsible for artwork/font provenance and version.

Files context menus use a native popup with a point-sized parent anchor. Keyboard
invocation locates the selected item, and pointer invocation retains its click
position. The menu can extend beyond the owner while the window system keeps it
on the output. Anchoring to the entire content item can make Qt place a native
popup beside the whole window instead. The release fixture checks keyboard
placement and a real right-click that crosses the owner's edge.
