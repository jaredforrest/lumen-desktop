#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-folder-model.XXXXXX | str trim)
cp ($repository | path join res lumen-desktop shell FolderStackModel.js) ($sandbox | path join FolderStackModel.js)
cp ($repository | path join tests desktop folder_stack_model.qml) ($sandbox | path join shell.qml)
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
} { ^timeout 10s quickshell -p $sandbox | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains 'PASS stack captured') or ($log | str contains 'FAIL ') {
    error make {msg: $'Folder stack QML model failed: ($log)'}
}
rm --recursive --force $sandbox
print 'PASS captured row/generation/folder/version, footer and uint64 strings'
