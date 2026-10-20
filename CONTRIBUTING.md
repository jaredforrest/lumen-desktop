# Development

Use the pinned environment and work in this repository. Desktop implementation changes do not belong in a consuming machine's configuration repository.

```sh
nix flake check --print-build-logs --option abort-on-warn true
nix build .#shell --out-link artifacts/shell-package --option abort-on-warn true
LUMEN_TEST_SHELL_LAUNCHER="$PWD/artifacts/shell-package/bin/lumen-desktop-shell" nix develop --command desktop-test-python tests/desktop/run_backend_tests.py --output artifacts/backends
```

`flake check` runs publication contracts and backend unit tests in the build sandbox. The integration command additionally exercises the Nix evaluator and QML helpers with private fixtures. Backend tests use temporary directories and private D-Bus sessions. A test must never connect to an actual account, edit a live profile, send a transfer or perform a session power action.

Render tests use a nested headless compositor. Build the shell, compositor and test runtime first; pass their actual immutable paths through the test command's arguments. `appearance_controls_regression.py`, `smoke_installed.py`, `calendar_popover_regression.py` and `auth_render.py` create their own fixtures and output logs/images under the requested fresh artifact directory. `localsend_render.py` additionally requires a private user/network namespace and the delayed pointer helper from `tests/desktop/click_pointer.nix`.

Do not treat a successful backend suite as visual certification. Keep render captures, capability checks, physical-device tests and actual deployment evidence distinct. Preserve warnings in test logs. Nix evaluations abort on warnings; inspect compiler logs before accepting a build.

Auth changes require isolated lock/greeter tests. File operations require cancellation/overwrite coverage. Host settings changes require the settings transaction tests. Never bypass a consumer's deployment or boot checks.

Hosted CI exercises both compositor variants. Disposable effects fixtures explicitly select Scenefx software rendering with `WLR_RENDERER_FORCE_SOFTWARE=1`; allowing software alone still tries to discover a DRM node. The development shell pins the Mesa EGL vendor so software DMA-BUF import and effect framebuffer support do not depend on host drivers. Software effects still need the Linux `udmabuf` allocator: hosted CI loads the running kernel’s module and grants only its runner account access to `/dev/udmabuf`. Missing module/device support fails the job with an explicit error; the workflow does not install or change kernels. The mandatory `effects_software_render.py` probe checks actual Scenefx shader/framebuffer initialization and changed corner pixels; the full effects smoke also checks rendered interiors. Production compositor settings are unchanged. To reproduce a GPU-less host locally, run the fixtures inside a user/mount namespace with an empty tmpfs mounted over `/dev/dri`; this never alters the host mount namespace.

The development shell contains the test tools and standalone settings evaluator, rather than the entire desktop application closure. Build `.#shell` explicitly for rendering tests and pass its launcher with `--launcher`.

Full-shell fixtures must copy assets into their fresh artifact directory and call
`tests/desktop/fixture_providers.py:isolate_providers(copied_shell, output)` before
launching the copied shell. A private session bus does not isolate direct provider
CLI/socket probes. The helper substitutes only the unrelated network and system
provider command targets with synthetic unavailable endpoints, records original
and modified hashes in `provider-isolation.json`, and logs every provider request.
Only `status` is accepted; other actions fail explicitly. It rejects external or
symlinked targets and changed command seams before writing either substitution.
The fixture still needs its private HOME/XDG, activation-free session bus, inert
system bus, and synthetic data. These tests do not certify actual provider or
hardware behavior. Fragment-only fixtures should instantiate only their required
owned components. Never add a production override to make a fixture safe.
