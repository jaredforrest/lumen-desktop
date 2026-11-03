# Hosted workflows paused

GitHub Actions workflows are temporarily disabled to avoid hosted runner usage.
Their complete definitions are retained as `*.yml.disabled`, which GitHub does
not register as workflows. Local Nix and native checks remain available.

Run the relevant checks locally, for example:

```console
nix build .#checks.x86_64-linux.source-contracts .#checks.x86_64-linux.offscreen-qml --no-link
```

The disabled definitions retain the complete hosted check commands for reference.
Native compositor checks must continue to use the existing isolated test-session
and runner-slot procedures.

To resume hosted checks, rename both `*.yml.disabled` files back to `*.yml` and
commit that change. Do not restore them until hosted usage is wanted again.
