# Disposable vault compatibility test

`tests/desktop/browser-workflows-rust/src/bin/disposable-vault.rs` exercises the packaged web client and CLI
against a private compatibility server. Test dependencies are optional flake
outputs and do not install or configure a desktop service.

The fixture enters separate user, process, network, mount, IPC and host-name
namespaces, detaches the original root and drops capabilities. Its source and
store mounts are read-only. The account, vault, browser profile, CLI session,
certificate authority and private keys live only in the private temporary home.
Only loopback networking is available. A bounded Rust HTTPS front end supplies HSTS;
Rust, Node and the browser verify a certificate trusted in that temporary
home. Browser diagnostics use inherited pipes. There is no host CA change,
certificate-verification override, hardware access or live vault access.

The server provisions a synthetic registration token through its supported
identity endpoint. The real web client derives the account keys and completes
password registration. The actual CLI then logs in, creates, reads and edits a
synthetic login, syncs, locks, refuses item access while locked, unlocks and reads
the edited item, trashes it, syncs and logs out. Credentials remain in memory
or the private home. A sentinel audit runs after child cleanup on success and
failure; a reflected credential or private key removes the offending material
and fails acceptance. Rust unit tests cover reflection, private key output,
nonregular artifacts, nested redaction and actual CLI timeout diagnostics.

Acceptance requires the Rust unit tests, the isolated transport receipt, and
the isolated `result.json` with credential audit. Runtime server warnings are
retained explicitly; CI builds use `abort-on-warn`.

This is backend and CLI compatibility coverage. Desktop password entry,
desktop lock/unlock, browser extension and autofill, biometric unlock, provider
outages, the user's actual backend and the user's full feature inventory still
require their own acceptance. A successful local server test does not establish
those capabilities or authorize migration of a user's vault.

`docs/vault-capabilities.json` records the implemented acceptance cases and the
capabilities that this suite does not establish. It defines scope; passing
receipts from a frozen run establish actual results for a pinned build.
