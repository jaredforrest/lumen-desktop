# Hosted namespace tests

The application fixtures require real unprivileged user, mount, process and
network namespaces. The actual namespace acceptance runs outside the Nix build
sandbox; pure rejection tests still run in the derivation. No namespace assertion
is skipped when a runner cannot provide it.

[Ubuntu's release notes](https://documentation.ubuntu.com/release-notes/24.04/)
describe its additional AppArmor restriction on capabilities within unprivileged
user namespaces. Nix-installed tools do not acquire distribution path profiles.
The integration job prepares a temporary profile attached to the single resolved,
immutable Nix `unshare` executable. It does not disable AppArmor or change global
namespace controls. The tool profile permits namespace use; it is not an
additional sandbox. The fixture itself establishes its disposable filesystem,
loopback-only network, process separation, non-root identity and capability drop.

Preparation refuses personal machines, self-hosted jobs, other distributions and
unvalidated executable paths. The exact executable and profile hashes and global
control snapshots are retained. The job removes the profile in an unconditional
cleanup step and checks the global controls again. The real namespace acceptance
still has to pass afterwards, including nested user namespaces used by browser
sandboxing. Permission to create namespaces never certifies fixture isolation.

This is a disposable runner prerequisite, not a desktop configuration change.
Local testing does not run the provisioning helper. On another runner image,
revalidate its security policy rather than weakening the fixture boundary.
