# Development

Use the pinned environment and work in this repository. Desktop implementation changes do not belong in a consuming machine's configuration repository.

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix develop --command desktop-test-python tests/desktop/run_backend_tests.py --output artifacts/backends
```

`flake check` runs publication contracts and backend unit tests in the build sandbox. The integration command additionally exercises the Nix evaluator and QML helpers with private fixtures. Backend tests use temporary directories and private D-Bus sessions. A test must never connect to an actual account, edit a live profile, send a transfer or perform a session power action.

Render tests use a nested headless compositor. Build the shell, compositor and test runtime first; pass their actual immutable paths through the test command's arguments. `appearance_controls_regression.py`, `smoke_installed.py`, `calendar_popover_regression.py` and `auth_render.py` create their own fixtures and output logs/images under the requested fresh artifact directory. `localsend_render.py` additionally requires a private user/network namespace and the delayed pointer helper from `tests/desktop/click_pointer.nix`.

Do not treat a successful backend suite as visual certification. Keep render captures, capability checks, physical-device tests and actual deployment evidence distinct. Preserve warnings in test logs. Nix evaluations abort on warnings; inspect compiler logs before accepting a build.

Auth changes require isolated lock/greeter tests. File operations require cancellation/overwrite coverage. Host settings changes require the settings transaction tests. Never bypass a consumer's deployment or boot checks.

Hosted CI builds both compositor variants and renders the shell through the basic compositor with Pixman. The effects compositor requires a DRM render node; its visual effects are checked on suitable local hardware rather than a GPU-less hosted runner.

The development shell contains the test tools and standalone settings evaluator, rather than the entire desktop application closure. Build `.#shell` explicitly for rendering tests and pass its launcher with `--launcher`.
