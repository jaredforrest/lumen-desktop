#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let fixture = ($repository | path join res lumen-desktop shell DisplaySpacesModelTest.qml)
let run = (with-env {
    QT_QPA_PLATFORM: offscreen
    QT_QUICK_BACKEND: software
    QT_ACCESSIBILITY: '0'
    NO_AT_BRIDGE: '1'
    GTK_A11Y: none
    QT_QPA_PLATFORMTHEME: generic
} { ^timeout 10s quickshell -p $fixture | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains DISPLAY_MODEL_PASS) or ($log | str contains DISPLAY_MODEL_FAIL) {
    error make {msg: $'Display model QML contract failed: ($log)'}
}
print 'PASS typed display authority, exact generation, legacy routing and disconnected Dock fallback'
