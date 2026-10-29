{ pkgs, testBusConfig }:
pkgs.runCommand "lumen-private-dbus-run-session" {
  nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
  LUMEN_DBUS_RUN_SESSION = "${pkgs.dbus}/bin/dbus-run-session";
  LUMEN_DBUS_TEST_CONFIG = testBusConfig;
} ''
  mkdir -p "$out/bin"
  rustc --edition=2024 -D warnings ${./private-bus.rs} -o "$out/bin/dbus-run-session"
  "$out/bin/dbus-run-session" -- ${pkgs.coreutils}/bin/printf '%s\n' true false 000 'a b' '"quoted"' > arguments
  printf '%s\n' true false 000 'a b' '"quoted"' > expected
  cmp arguments expected
''
