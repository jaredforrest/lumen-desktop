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
is not a Windows target, and loopback is not a routed network test. Clipboard,
fullscreen escape, capture of system shortcuts, automatic reconnection, routed
networks, Windows interoperability and a complete application-workflow acceptance
remain separate evidence requirements.
