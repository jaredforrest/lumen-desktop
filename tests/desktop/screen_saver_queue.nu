#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let fixture_package = (^nix build ($repository + '#test-desktop-policy-fixture') --no-link --print-out-paths | lines | last)
let fixture_binary = ($fixture_package | path join bin lumen-desktop-policy-fixture)
let sandbox = (^mktemp -d -t lumen-saver-queue.XXXXXX | str trim)
cp ($repository | path join res lumen-desktop shell ScreenSaverState.qml) ($sandbox | path join ScreenSaverState.qml)
cp ($repository | path join tests desktop screen_saver_queue.qml) ($sandbox | path join shell.qml)
mkdir ($sandbox | path join bin)
^ln -s $fixture_binary ($sandbox | path join bin lumen-screen-saver)
for directory in [home runtime state config cache data] {
    let target = ($sandbox | path join $directory)
    mkdir $target
    ^chmod 700 $target
}
let run = (with-env {
    HOME: ($sandbox | path join home)
    XDG_RUNTIME_DIR: ($sandbox | path join runtime)
    XDG_STATE_HOME: ($sandbox | path join state)
    XDG_CONFIG_HOME: ($sandbox | path join config)
    XDG_CACHE_HOME: ($sandbox | path join cache)
    XDG_DATA_HOME: ($sandbox | path join data)
    PATH: ($env.PATH | prepend ($sandbox | path join bin))
    LUMEN_TEST_FIXTURE_BIN: $fixture_binary
    LUMEN_TEST_SAVER_QUEUE_ROOT: $sandbox
    QT_QPA_PLATFORM: offscreen
    QT_QPA_PLATFORMTHEME: generic
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
} { ^timeout 15s quickshell -p $sandbox | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains QUEUE_PASS) or ($log | str contains QUEUE_FAIL) or ($log | str contains ReferenceError) or ($log | str contains TypeError) {
    error make {msg: $'Screen saver QML queue failed: ($log)'}
}
let requests = (open --raw ($sandbox | path join requests.jsonl) | lines | where {|line| $line != ''} | each {|line| $line | from json})
let expected = [
    {action: status}
    {action: set, key: enabled, value: false}
    {action: set, key: idleSeconds, value: 90}
    {action: set, key: motion, value: false}
]
if $requests != $expected { error make {msg: $'Screen saver requests mismatched: ($requests | to json --raw)'} }
rm --recursive --force $sandbox
print 'PASS actual Qt screen saver queue preserves explicit settings behind held status request'
