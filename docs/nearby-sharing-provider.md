# Nearby sharing provider boundary

The desktop supplies the Control Centre tile and Files menu. A consumer supplies
the label and optional transport/UI package through
`services.lumen-desktop.integrations.nearbySharing.{label,providerPackage}`.
The default label is `Nearby Sharing`; the default package is null. No receiver,
radio policy, account, firewall rule or transport is installed or started by
these options. Existing optional transport frontends remain consumer-selected;
the core controls never name a machine-specific backend.

The package exposes `bin/lumen-nearby-provider`. `status` is read-only and emits
one JSON object such as
`{"available":true,"active":false,"canSend":true,"detail":"Off"}`.
Both available and active must be booleans. Missing canSend means false; an
unavailable provider cannot advertise active reception or sending. Detail is
plain text, at most 160 characters, without control characters. Do not include
credentials, sender identities, file paths or other sensitive state in status.
Unknown fields are discarded.

`open` displays the provider's receiving controls; it does not imply consent to
start a receiver. `send PATH...` opens the provider's explicit sending flow for
one to 128 absolute local paths. The desktop passes literal argument vectors,
never a shell expression. The provider owns selection confirmation, capture of
file identity, consent, transfer integrity and error reporting. The router
rechecks the sending capability before dispatch. This interface does not claim
interoperability, authenticated peer identity or successful delivery.

`lumen-nearby-sharing status` validates the configured immutable executable and
bounds collection, EOF/wait and decoding to two seconds and 16 KiB combined
output. A failure, malformed response or any diagnostic returns unavailable
without exposing raw output. The owned status process group is retired on success and failure before its
leader is reaped, with a separate one-second cleanup bound. No configuration means no
provider process. Opening and sending happen only on explicit user actions.

The shared QML state expires after 7.5 seconds, rejects backwards-clock freshness
and clears failed responses. Control Centre polls while visible; opening the
Files menu requests a fresh observation. Unconfigured, stale and receiving-only
states cannot enable sending. Consumer branding stays in consumer configuration.

Opening the Files menu captures the folder, tab, ordered selection and displayed
file identities. Sending rechecks those presentation facts and current provider
capability, then passes the captured literal paths. A changed selection or row
requires reopening the menu; it cannot silently substitute another file. This
does not pin external file reads: the provider still owns filesystem validation
and its explicit confirmation flow.

`test_file_share_intent.py` verifies lossless native identities, captured literal
arguments and stale/invalid selection refusal. `files_share_menu_qml.py` opens
the actual Files menu in the private offscreen boundary and activates its real
row signal. It proves unchanged selection dispatch, replacement refusal and a
fresh explicit menu dispatch. Only volume discovery, sharing status and external
launch are replaced by hashed private observers; no transfer is attempted.
The optional `--window-source` accepts an immutable earlier Files window for
reproducing the pre-fix substitution, adding only observer object names.
