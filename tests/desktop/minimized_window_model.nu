#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-minimized-model.XXXXXX | str trim)
for name in [MinimizedWindowState.qml DockModel.js DockInteraction.js] {
    cp ($repository | path join res lumen-desktop shell $name) ($sandbox | path join $name)
}
cp ($repository | path join tests desktop minimized_window_model.qml) ($sandbox | path join shell.qml)
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
    QT_QPA_PLATFORM: offscreen
    QT_QPA_PLATFORMTHEME: generic
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
} { ^timeout 12s quickshell -p $sandbox | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains 'PASS retained minimized capabilities') or ($log | str contains 'Fontconfig error:') {
    error make {msg: $'Minimized window QML model failed: ($log)'}
}
rm --recursive --force $sandbox
print 'PASS retained minimized capabilities, live bindings, stale pointer/replacement refusal and application default'
