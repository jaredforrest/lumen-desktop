#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let package = (^nix build ($repository + '#test-desktop-policy-fixture') --no-link --print-out-paths | lines | last)
let stub = ($package | path join bin files_listing_checkpoint_stub)
let sandbox = (^mktemp -d -t lumen-list-checkpoint.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join assets)
cp ($repository | path join tests desktop files_listing_checkpoint.qml) ($sandbox | path join shell.qml)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let run = (with-env {
    HOME: ($sandbox | path join home)
    XDG_RUNTIME_DIR: ($sandbox | path join runtime)
    XDG_STATE_HOME: ($sandbox | path join state)
    XDG_CONFIG_HOME: ($sandbox | path join config)
    XDG_CACHE_HOME: ($sandbox | path join cache)
    XDG_DATA_HOME: ($sandbox | path join data)
    QT_QPA_PLATFORM: offscreen
    QT_QPA_PLATFORMTHEME: generic
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
    LUMEN_FILES_BIN: $stub
    DBUS_SESSION_BUS_ADDRESS: ($'unix:path=($sandbox)/no-session')
    DBUS_SYSTEM_BUS_ADDRESS: ($'unix:path=($sandbox)/no-system')
} { ^timeout 12s quickshell -p ($sandbox | path join shell.qml) | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains 'FILES_CHECKPOINT_PASS') or ($log | str contains 'FILES_CHECKPOINT_FAIL') or ($log | str contains 'ReferenceError') or ($log | str contains 'TypeError') {
    error make {msg: $'Files listing checkpoint failed: ($log)'}
}
rm --recursive --force $sandbox
print 'PASS actual current, complete, stale, error and path-change listing authority transport'
