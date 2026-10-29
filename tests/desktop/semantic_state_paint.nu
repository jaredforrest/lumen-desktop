#!/usr/bin/env nu
def luminance [color] {
    let channels = ([r g b] | each {|key|
        let value = ($color | get $key)
        if $value <= 0.04045 { $value / 12.92 } else { (($value + 0.055) / 1.055) ** 2.4 }
    })
    ($channels.0 * 0.2126 + $channels.1 * 0.7152 + $channels.2 * 0.0722)
}
def contrast-ratio [a b] {
    let one = (luminance $a)
    let two = (luminance $b)
    (([$one $two] | math max) + 0.05) / (([$one $two] | math min) + 0.05)
}
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let appearance_package = (^nix build ($repository + '#appearance-runtime') --no-link --print-out-paths | lines | last)
let appearance_binary = ($appearance_package | path join bin lumen-appearance)
let sandbox = (^mktemp -d -t lumen-semantic-paint.XXXXXX | str trim)
cp -r ($repository | path join res lumen-desktop shell) ($sandbox | path join assets)
for directory in [home runtime state config cache data] {
    let path = ($sandbox | path join $directory)
    mkdir $path
    ^chmod 700 $path
}
let palette = (open ($repository | path join res lumen-desktop appearance palette.json))
{font: $palette.font, palette: $palette.light, preferences: {reduceMotion: true}} | to json | save ($sandbox | path join appearance.json)
mut cases = []
for mode in [light dark] {
    for high_contrast in [false true] {
        let colors = (^$appearance_binary --resolve-palette $mode blue ($high_contrast | into string) | from json)
        for width in [580 220] {
            $cases = ($cases | append {mode: $mode, contrast: $high_contrast, width: $width, colors: $colors})
        }
    }
}
let qml = ($sandbox | path join shell.qml)
(open --raw ($repository | path join tests desktop semantic_state_paint.qml) | str replace '__CASES__' ($cases | to json --raw)) | save $qml
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
    LUMEN_SEMANTIC_CAPTURE_DIR: $sandbox
    LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
} { ^timeout 20s quickshell -p $qml | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or ($log | str contains SEMANTIC_PAINT_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) {
    error make {msg: $'Semantic QML capture failed: ($log)'}
}
let receipt = ($log | lines | where {|line| $line | str contains 'SEMANTIC_PAINT_RESULT '})
if ($receipt | length) != 1 { error make {msg: $'Missing semantic paint receipt: ($log)'} }
let cases = ($receipt | first | split row 'SEMANTIC_PAINT_RESULT ' | last | from json)
if ($cases | length) != 8 { error make {msg: 'Expected eight semantic theme, contrast and width cases'} }
for case in $cases {
    if $case.checkedThumb != 14 or not $case.disabled or not ($case.radio | str contains Connected) {
        error make {msg: 'Non-hue switch, radio or disabled state missing'}
    }
    for row in $case.statuses {
        let ratio = (contrast-ratio $row.foreground $row.background)
        if $ratio < 4.5 { error make {msg: $'Status contrast below 4.5:1: ($case.mode), ($row.severity), ($ratio)'} }
        if $row.severity != info {
            let prefix = ($row.severity | str capitalize) + ':'
            if $row.padding <= 0 or $row.padding >= ($row.rect.width / 2) or not ($row.visibleTexts | any {|text| $text | str starts-with $prefix}) {
                error make {msg: $'Visible non-hue status label missing: ($row.severity)'}
            }
        }
        let rect = $row.rect
        let x = (($rect.x * 2) | math round | into int)
        let y = (($rect.y * 2) | math round | into int)
        let width = (($rect.width * 2) | math round | into int)
        let height = (($rect.height * 2) | math round | into int)
        let histogram = (^magick $case.capture -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
        mut ink = 0
        for line in $histogram {
            let parsed = ($line | parse -r 'srgba?\((?<r>\d+),(?<g>\d+),(?<b>\d+)(?:,[^)]+)?\)')
            if ($parsed | is-empty) { continue }
            let pixel = ($parsed | first)
            let actual = {r: (($pixel.r | into int) / 255), g: (($pixel.g | into int) / 255), b: (($pixel.b | into int) / 255)}
            let distance = ([r g b] | each {|key| (((($actual | get $key) - ($row.foreground | get $key)) * 255) | math abs)} | math max)
            if $distance <= 3 { $ink += ($line | split row ':' | first | str trim | into int) }
        }
        if $ink < 10 { error make {msg: $'Status text was not painted: ($case.mode), ($row.severity), pixels=($ink)'} }
    }
}
rm --recursive --force $sandbox
print 'PASS eight production semantic states with painted text contrast and visible non-hue labels'
