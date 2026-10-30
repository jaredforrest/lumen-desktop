# Remote client compatibility fixtures

`lumen-remote-client-fixture` drives the packaged Remmina client through the public
remote-desktop adapter, inside a disposable user, PID, mount, IPC and network
namespace. It reads only synthetic profiles. The namespace has loopback, private
D-Bus and accessibility services, a private secret service and null audio. It
never exposes host account services, credentials, display sockets or network
peers. Private PTYs are enabled only for the SSH case.

The finite protocol matrix is:

| Protocol | Actual target | Authentication | Identity negative | Recovery |
| --- | --- | --- | --- | --- |
| VNC | WayVNC, second headless output | VeNCrypt X509Plain with protected credential entry | Trusted CA with wrong IP SAN, rejected before credentials | Kill server; restart unchanged certificate; reconnect same profile |
| RDP | FreeRDP shadow, private software X target | NLA with private SAM database | Trusted CA with wrong DNS identity requires approval before credentials | Kill server; restart unchanged certificate; reconnect same profile |
| SSH | OpenSSH, private PTY shell | Ed25519 identity file | Changed pre-pinned host key requires approval before authentication | Kill owned connection process tree; reconnect same pinned profile |

Each protocol also rejects a wrong password or identity key. VNC and RDP verify
four actual framebuffer color regions, including after reconnect. RDP uses an
explicit 32-bit software mode and a nondefault port. SSH sends physical keyboard
input through the actual client terminal and checks a receipt produced by the
remote shell, including after reconnect. Reconnect is explicitly a manual
same-profile client restart; these tests do not prove automatic reconnection.

The adapter's profile-list operation is checked to leave profile bytes unchanged.
The real client can persist its normal preferences and credentials in the private
fixture home. The fixture guards generated credentials and encoded equivalents
against appearing in retained artifacts; all private key material stays inside
the disposable home.

The `remote-peer-identity` flake check uses Rust 2024 through the C ABI to test
the actual patched upstream OpenSSL verifier. It checks exact IPv4/IPv6 identity
bytes, DNS identity and invalid parameters. Native acceptance uses the actual
Rust remote-desktop adapter and a pinned executable destination, preserving its
profile parsing and dispatch behavior. The target is an actual GTK program
implemented through Rust FFI; no legacy script runtime is used.

The public client package applies narrow pinned-source repairs: numeric VNC
addresses use IP SAN verification; RDP certificate names exclude the connection
port; newly unspecified SSH policies default to strict host checking; windowed
connections do not access an absent floating toolbar. Existing explicitly stored
SSH preferences remain authoritative and no user profile is rewritten.

All provider diagnostics are retained after child teardown. A successful protocol
assertion does not mean third-party logs are warning-free. The Linux RDP target
is not a Windows target, and loopback is not a routed network test.

The separate `interactions` workflow proves physical text and function-key input,
viewer fullscreen entry and exit through an explicitly configured F12+f shortcut,
and plain-text clipboard transfer in both directions. VNC uses two independent
compositors, display sockets and seats; RDP uses an independent Xvfb display with
an actual xrdp TLS frontend, authenticated x11vnc backend and xrdp-chansrv. This
RDP clipboard case proves TLS, separately from the shadow-server NLA cases.
Fullscreen can recreate the provider window, so the fixture records actual
window identities and refocuses its accessible drawing surface.

The `text-clipboard` workflow uses actual GTK text documents and physical
Copy/Paste, with Latin diacritics, Greek, CJK, RTL scripts, supplementary emoji,
combining sequences, tabs, blank lines and trailing newlines. Raw source and
destination strings are retained. VNC requires exact UTF-8 source bytes both
ways. RDP requires exact source bytes remote-to-local; local-to-remote permits
only the complete LF-to-CRLF transformation required by
[CF_UNICODETEXT](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-dclb/53128a2c-84ad-464e-8a12-75c5da932237).
It does not trim, normalize Unicode, tolerate mixed line endings or discard
whitespace; the receipt explicitly reports whether source bytes were exact.

The public VNC client registers the upstream UTF-8 callback, sends extended
clipboard text through its supported API, and requests deferred selections only
when the peer advertises Request support. Its legacy path converts representable
Latin-1 text losslessly and displays an explicit warning instead of replacing
unsupported characters. The `legacy-clipboard` workflow uses stock x11vnc, which
registers only the legacy cut-text callback. It proves representable fallback,
visible unsupported-text rejection, unchanged remote text and process identity,
and continued physical input. This legacy test uses unencrypted RFB only inside
the private loopback namespace; it is not an encrypted transport acceptance.

HTML/files clipboard formats, default shortcut policy, capture of system-wide
host shortcuts, automatic reconnection, routed networks, Windows interoperability
and complete application-workflow acceptance remain separate requirements.
