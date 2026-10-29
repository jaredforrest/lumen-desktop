{ pkgs, testBusConfig }:
pkgs.runCommand "lumen-private-dbus-run-session" {
  nativeBuildInputs = [ pkgs.rustc pkgs.stdenv.cc ];
  LUMEN_DBUS_RUN_SESSION = "${pkgs.dbus}/bin/dbus-run-session";
  LUMEN_DBUS_DAEMON = "${pkgs.dbus}/bin/dbus-daemon";
  LUMEN_DBUS_TEST_CONFIG = testBusConfig;
} ''
  mkdir -p "$out/bin"
  rustc --edition=2024 -D warnings ${./private-bus.rs} -o "$out/bin/dbus-run-session"
  rustc --edition=2024 -D warnings --test ${./private-bus.rs} -o bus-options-test
  ./bus-options-test
  "$out/bin/dbus-run-session" -- ${pkgs.coreutils}/bin/printf '%s\n' true false 000 'a b' '"quoted"' --config-file=child --dbus-daemon=child > arguments
  printf '%s\n' true false 000 'a b' '"quoted"' --config-file=child --dbus-daemon=child > expected
  cmp arguments expected
''
