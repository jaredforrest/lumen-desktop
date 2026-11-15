# Wi-Fi passwords in Bitwarden

Enable `services.lumen-desktop.integrations.wifiVault.enable` on a consumer host
using NetworkManager. This installs a user timer that retries every five minutes;
the Wi-Fi menu also exposes Unlock & Sync and Sync with Bitwarden.

Bitwarden's `Wi-Fi` folder owns enrolled WPA-Personal passwords. Each entry has a
normal password field and a `lumen.wifi.v1:` JSON note identifying its SSID and
security mode. Only marked entries in this folder are read; unrelated vault
items are never exported. Enterprise and certificate-based networks are excluded.
Passwords with leading or trailing whitespace are also excluded because the
NetworkManager editor strips it; they remain local rather than being silently
changed. Enrolled vault items using such values stop sync with a validation error.

An initial sync enrolls supported saved NetworkManager profiles that have no
matching vault entry. A matching vault entry always wins over a different cached
password. To rotate a password, change its password field in Bitwarden and sync;
do not remove its identifying note. Duplicate or malformed entries stop syncing
rather than choosing an arbitrary credential. Sync never deletes profiles or
vault items, never activates a connection, and never reconnects the current link.

A newly joined network is saved by NetworkManager first. Sync runs afterward;
a locked vault or offline server does not turn a successful connection into a
failure. NetworkManager's normal saved profile is the pending cache, with no
additional plaintext password queue. Once unlocked and online, the timer uploads
new entries and refreshes cached passwords. Cached Wi-Fi remains available before
vault unlock and after reboot. Bitwarden updates are not guaranteed to arrive
while the vault is locked.

Passwords pass over process stdin and captured output, never command arguments,
logs, Git, the Nix store or status reports. NetworkManager retains its normal
protected connection files. The local status file contains only counts, a time
and a generic result. The standard encrypted rbw vault cache is retained by rbw.

## New-device email verification

The packaged rbw 1.15.0 cannot complete Bitwarden's new-device email verification
(upstream issue 318). The Wi-Fi helper uses the narrowly scoped upstream fix from
https://github.com/doy/rbw/pull/319, pinned to
2485ba6a3860f8e13ca56cf83a9f002e7342331e. It adds the code prompt and the server's
`newDeviceOtp` field; it does not disable verification. This is an unmerged
upstream patch, kept separately in `packages/wifi-vault-rbw.nix` for removal when
a released client supports the flow. An already running old rbw agent must be
stopped once before starting this client. Login and code entry happen in a
separate local terminal; neither belongs in chat.

## Validation

The system-controls package runs isolated fake-provider tests for vault-winning
conflicts, first-time enrollment and locked-vault offline operation. Its unit
tests also reject control characters, unsupported security types and invalid
credentials. Live rollout should verify the enrolled item count and perform a
second idempotent sync without disconnecting Wi-Fi.
