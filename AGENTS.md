# Repository instructions

Before continuing paused work, inspect the current branch, `git status`, and recent commits in this worktree. Use the current checkout as the source of truth; earlier chat context may describe files that have since been migrated. Do not restore superseded files from an older checkpoint or finish work in another checkout without integrating it here.

First-party implementation and test code uses Rust 2024, Nushell, and Nix. Keep QML and its embedded JavaScript where Qt requires them, the scoped Electron adapter JavaScript, C for kernel or driver interfaces, browser test pages, and upstream patches. Do not add first-party Python, C++, or other script languages. When changing a fixture, migrate its existing behavior and remove its superseded source.

Before claiming a migration or refactor complete, run `nix build .#checks.x86_64-linux.source-contracts --no-link` and the relevant build or behavioral tests, then verify this worktree is the one containing the result.
