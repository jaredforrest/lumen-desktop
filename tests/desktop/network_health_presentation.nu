#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-network-qt.XXXXXX | str trim)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
cp ($repository | path join res lumen-desktop shell NetworkState.js) ($sandbox | path join NetworkState.js)
let qml = ($sandbox | path join shell.qml)
(open --raw ($repository | path join tests desktop network_health_presentation.qml)) | save $qml
let run = (with-env {
    HOME: ($sandbox | path join home)
    XDG_RUNTIME_DIR: ($sandbox | path join runtime)
    XDG_STATE_HOME: ($sandbox | path join state)
    XDG_CONFIG_HOME: ($sandbox | path join config)
    XDG_CACHE_HOME: ($sandbox | path join cache)
    XDG_DATA_HOME: ($sandbox | path join data)
    DBUS_SYSTEM_BUS_ADDRESS: $'unix:path=($sandbox)/no-system-bus'
    QT_QPA_PLATFORM: offscreen
    QT_QPA_PLATFORMTHEME: generic
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
} { ^timeout 15s quickshell -p $qml | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains NETWORK_HEALTH_PRESENTATION_PASS) {
    error make {msg: $'Network presentation Qt test failed: ($log)'}
}
for marker in [NETWORK_HEALTH_PRESENTATION_FAIL 'TypeError:' 'ReferenceError:' 'Binding loop'] {
    if ($log | str contains $marker) { error make {msg: $'Network presentation Qt warning: ($log)'} }
}
rm --recursive --force $sandbox
print 'PASS absent, partial, malformed and populated optional network records'
