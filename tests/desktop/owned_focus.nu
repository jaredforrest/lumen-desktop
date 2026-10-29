#!/usr/bin/env nu
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let sandbox = (^mktemp -d -t lumen-focus-qt.XXXXXX | str trim)
let assets = ($sandbox | path join assets)
cp -r ($repository | path join res lumen-desktop shell) $assets
for directory in [home runtime state config cache data] {
    let target = ($sandbox | path join $directory)
    mkdir $target
    ^chmod 700 $target
}
let palettes = (open ($repository | path join res lumen-desktop appearance palette.json))
let qml = ($sandbox | path join shell.qml)
(open --raw ($repository | path join tests desktop owned_focus.qml) | str replace '__PALETTES__' ($palettes | to json --raw)) | save $qml
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
    LUMEN_UNIFIED_OWNED_HEADERS: '1'
    LUMEN_NATIVE_WINDOW_CONTROLS: '0'
    LUMEN_FOCUS_CAPTURE_DIR: $sandbox
} { ^timeout 20s quickshell -p $qml | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or ($log | str contains OWNED_FOCUS_FAIL) or ($log | str contains ReferenceError) or ($log | str contains TypeError) {
    error make {msg: $'Focus QML failed: ($log)'}
}
let receipt = ($log | lines | where {|line| $line | str contains 'OWNED_FOCUS_RESULT '})
if ($receipt | length) != 1 { error make {msg: $'Missing focus receipt: ($log)'} }
let rows = ($receipt | first | split row 'OWNED_FOCUS_RESULT ' | last | from json)
if ($rows | length) != 8 { error make {msg: 'Expected eight theme, transparency and production control cases'} }
for index in 0..7 {
    let row = ($rows | get $index)
    let rect = $row.bounds
    let width = ($rect.width | math round | into int)
    let height = ($rect.height | math round | into int)
    let x = ($rect.x | math round | into int)
    let y = ($rect.y | math round | into int)
    if $width < 1 or $height < 1 { error make {msg: $'Empty focus bound ($index)'} }
    let crop = $'($width)x($height)+($x)+($y)'
    let ordinary = ($sandbox | path join $'($index)-ordinary.png')
    let focused = ($sandbox | path join $'($index)-focused.png')
    let before = ($sandbox | path join $'($index)-ordinary-crop.png')
    let after = ($sandbox | path join $'($index)-focused-crop.png')
    ^magick $ordinary -crop $crop +repage $before
    ^magick $focused -crop $crop +repage $after
    let changed = (^magick $before $after -compose difference -composite -colorspace Gray -threshold '30%' -format '%[fx:mean*w*h]' info: | str trim | into float)
    if $changed < 4.0 { error make {msg: $'Production focus paint did not change at least four high-delta pixels: ($index), ($row.entry)'} }
}
rm --recursive --force $sandbox
print 'PASS actual production close and File Info focus paint in eight light/dark and transparency cases'
