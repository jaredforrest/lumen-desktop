{ pkgs }:
let
  buildOwnedRust = import ../../../packages/owned-rust.nix { inherit pkgs; };
  base = {
    pname = "lumen-rust-warning-policy-fixture";
    version = "0.1.0";
    src = ./.;
    cargoLock.lockFile = ./Cargo.lock;
  };
  direct = buildOwnedRust (base // { RUSTFLAGS = ''-Awarnings --cfg feature="preserved"''; });
  nested = buildOwnedRust (base // { env.RUSTFLAGS = [ "-Awarnings" "--cfg" ''feature="preserved"'' ]; });
  separator = builtins.fromJSON ''"\u001f"'';
  encoded = buildOwnedRust (base // {
    env.CARGO_ENCODED_RUSTFLAGS = "-Awarnings" + separator + "--cfg" + separator + ''feature="preserved"'';
  });
  negative = drv: pkgs.testers.testBuildFailure' {
    drv = drv.overrideAttrs { cargoBuildFeatures = [ "warning" ]; };
    expectedBuilderExitCode = 101;
    expectedBuilderLogEntries = [ "unused variable: `policy_fixture_unused`" "-D warnings" ];
  };
  rejectsOverride = value: !(builtins.tryEval (builtins.deepSeq
    (buildOwnedRust (base // value)).drvPath true)).success;
in
assert rejectsOverride { RUSTFLAGS = "--cap-lints allow"; };
assert rejectsOverride { env.CARGO_ENCODED_RUSTFLAGS = "--cap-lints=allow"; };
assert rejectsOverride { env.RUSTFLAGS = "--force-warn unused-variables"; };
assert rejectsOverride { CARGO_ENCODED_RUSTFLAGS = "--force-warn=unused-variables"; };
pkgs.linkFarm "lumen-owned-rust-warning-policy" [
  { name = "direct-positive"; path = direct; }
  { name = "env-positive"; path = nested; }
  { name = "encoded-positive"; path = encoded; }
  { name = "compiler-warning-negative"; path = negative direct; }
  { name = "encoded-compiler-warning-negative"; path = negative encoded; }
  { name = "owned-dependency-warning-negative"; path = pkgs.testers.testBuildFailure' {
    drv = direct.overrideAttrs { cargoBuildFeatures = [ "dependency-warning" ]; };
    expectedBuilderExitCode = 101;
    expectedBuilderLogEntries = [ "unused variable: `policy_dependency_unused`" "-D warnings" ];
  }; }
]
