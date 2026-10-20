# Fingerprint reader status

Privacy & Security can inspect an already running fingerprint provider. It uses
the [provider's GetDevices API](https://fprint.freedesktop.org/fprintd-dev/Manager.html)
through its captured unique bus owner. Only a root-owned provider is accepted.
Discovery never starts a service, claims hardware, lists enrollment, prompts for
authorization, enrolls, verifies or changes PAM. Provider absence remains unknown;
an empty successful reply means only that this provider reports no reader.

A reported reader does not prove enrollment, login policy or functioning unlock.
The existing host-managed password path is unchanged by discovery. Actual
biometric authentication acceptance requires supported enrolled hardware and a
separate review of the host's authentication configuration. This status does not
enable biometric authentication or certify password fallback for every PAM stack.

The observation has a 1.5-second bus-call budget, a separate 2.5-second subprocess
boundary and bounded output. Owner replacement, late replies, denied discovery or
malformed data produce an unavailable observation. Device paths and identities
are not displayed or retained. Refresh rechecks the current provider.
