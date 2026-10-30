# Owned Rust compiler diagnostics

Repository-owned Rust runtimes and test fixtures use `packages/owned-rust.nix`,
a small wrapper around the pinned standard `rustPlatform.buildRustPackage`.
The source contract rejects new owned derivations that call the underlying
builder directly. This includes the character-viewer runtime and private native
test fixtures, rather than only packages exported as top-level flake attributes.
Standalone owned `rustc` invocations, including compilers launched inside test
processes, pass the same explicit warning denial. The source contract checks the
repository's direct command and `Command::args` forms to prevent accidental
omissions; the offscreen QML check actually compiles and runs those fixtures.

The builder appends `-D warnings` to compiler and documentation flags while
preserving existing direct or `env` flags. Encoded Cargo flags take precedence
over ordinary flags, so the policy also appends the denial to those when present.
Package-supplied `--cap-lints` and `--force-warn` are rejected because they could
downgrade the policy. Existing standard Cargo treatment of registry dependencies is unchanged:
this policy does not globally change upstream packages or claim that their
diagnostics are warning-free. The three pinned office providers retain their
existing separate compiler policy and upstream source boundary.

`checks.x86_64-linux.owned-rust-warning-policy` builds clean fixture crates using
direct, environment and encoded flags. Their source requires an existing custom
configuration flag, proving it survived the wrapper. A preexisting `-Awarnings`
also proves the final denial takes precedence. Standard Nix build-failure
testers then compile the same crate with a genuine unused-variable warning and
require Cargo's failure plus the exact compiler diagnostic, including with
encoded flags. Expected failures are preserved as test artifacts; another build
failure cannot masquerade as the intended negative result.
An additional actual compiler negative puts the warning in an owned local path
dependency, proving that this source boundary also receives fatal diagnostics.

`abort-on-warn` remains useful for Nix evaluation diagnostics, but it does not
replace the Rust policy. Runtime diagnostics, external provider builds and direct
developer Cargo invocations remain separate boundaries. Validation commands use
two build jobs and two cores; no native compositor or host session is needed.
This is a reproducible project build contract, not a security boundary against
consumers changing their own source or build policy.
