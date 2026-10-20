# Lumen Desktop

An independent Linux desktop project built with Quickshell, Qt Quick and Python, with a patched Sway/SwayFX compositor and Nix packaging. This is an early extraction from a working personal system, with substantial integration work still ahead.

The project owns the shell, window interaction, desktop services, toolkit adapters and regression tests. A consuming NixOS configuration owns users, hardware, storage, secrets, backup and deployment policy. No machine configuration is included here.

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix develop
```

The default package is `nix build .#shell`; the patched compositor is `nix build .#compositor`. Running the shell requires an existing compatible Wayland session. Installing a package does not install a login session or configure authentication.

The NixOS module requires Home Manager, an existing Wayland configuration, a desktop user and a settings command. See [integration](docs/integration.md), [architecture](docs/architecture.md) and [contributing](CONTRIBUTING.md).

The default standalone settings adapter uses immutable defaults. A host injects its own adapter to enable durable settings edits; this project does not own or authorize a host's privileged rebuild command.

Public source does not yet carry a blanket redistribution licence. Original geometric artwork is included; fetched dependencies retain their own licences. A project-wide licence needs to be chosen before presenting this as a generally reusable distribution.

Status order and clock formatting: [presentation contract and tests](docs/status-clock.md).
Saved portal decisions: [provider contract, scope and acceptance](docs/portal-permissions.md).

Interface text sizing: [scope, layout contract and tests](docs/text-sizing.md).

Per-device input preferences: [supported capabilities, identity scope and tests](docs/input-preferences.md).
Window Groups provides optional native presentation groups; see [the workflow and current boundaries](docs/window-groups.md).

[Application desktop assignment](docs/application-desktops.md) documents supported membership and native acceptance.

[Live transparency acceptance](docs/transparency-testing.md) documents mapped-surface geometry, paint and input checks.

[Mixed output scaling](docs/mixed-output-scale-testing.md) describes native geometry, painted-frame and input acceptance.

[Content clipping](docs/content-clipping-testing.md) covers declared masks, vivid edge pixels and preserved production input targets.

File identity serialization and compatibility are documented in [file identity wire format](docs/file-identity-wire.md).

The [icon catalogue acceptance](docs/icon-catalogue-testing.md) documents reproducible scale and symbol rendering checks.
