{ pkgs }:
args:
let
  lib = pkgs.lib;
  environment = args.env or { };
  flags = value: if builtins.isList value then lib.concatStringsSep " " value else value;
  separator = builtins.fromJSON ''"\u001f"'';
  warningFlags = name: encodedName:
    let
      original = lib.concatStringsSep " " (builtins.filter (value: value != "") [
        (flags (args.${name} or ""))
        (flags (environment.${name} or ""))
      ]);
      encoded = lib.concatStringsSep separator (builtins.filter (value: value != "") [
        (args.${encodedName} or "")
        (environment.${encodedName} or "")
      ]);
    in
    assert lib.assertMsg (!(lib.any (flag: lib.hasInfix flag (original + encoded)) [ "--cap-lints" "--force-warn" ]))
      "Owned Rust flags cannot override the compiler warning policy";
    { ${name} = original + " -D warnings"; }
    // lib.optionalAttrs (encoded != "") { ${encodedName} = encoded + separator + "-Dwarnings"; };
in
pkgs.rustPlatform.buildRustPackage ((builtins.removeAttrs args [
  "RUSTFLAGS" "RUSTDOCFLAGS" "CARGO_ENCODED_RUSTFLAGS" "CARGO_ENCODED_RUSTDOCFLAGS"
]) // {
  env = environment
    // warningFlags "RUSTFLAGS" "CARGO_ENCODED_RUSTFLAGS"
    // warningFlags "RUSTDOCFLAGS" "CARGO_ENCODED_RUSTDOCFLAGS";
  passthru = (args.passthru or { }) // {
    lumenOwnedRustWarnings = true;
  };
})
