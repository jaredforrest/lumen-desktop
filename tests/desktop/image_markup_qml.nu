#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-markup-qt.XXXXXX | str trim)
let assets = ($sandbox | path join assets)
cp -r ($repository | path join res lumen-desktop shell) $assets
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let image = ($sandbox | path join original.png)
^magick -size 80x60 xc:white $image
let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
let appearance = ($sandbox | path join appearance.json)
{font: $palette.font, palette: $palette.dark, preferences: {}} | to json | save $appearance
let qml = ($sandbox | path join shell.qml)
(open --raw ($repository | path join tests desktop image_markup_qml.qml) | str replace --all '__IMAGE_PATH__' $image) | save $qml
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
    LUMEN_APPEARANCE_STATE: $appearance
} { ^timeout 15s quickshell -p $qml | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or not ($log | str contains IMAGE_MARKUP_QT_PASS) {
    error make {msg: $'Image markup Qt test failed: ($log)'}
}
for marker in [IMAGE_MARKUP_QT_FAIL 'TypeError:' 'ReferenceError:' 'Binding loop' 'Cannot assign' 'Failed to load configuration' 'Could not attach Keys'] {
    if ($log | str contains $marker) { error make {msg: $'Image markup Qt warning: ($log)'} }
}
rm --recursive --force $sandbox
print 'PASS actual Qt markup intent, cancellation, stale reply and selection change'
