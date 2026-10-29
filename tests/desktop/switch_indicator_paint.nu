#!/usr/bin/env nu
def luminance [color] {
    let channels = ([r g b] | each {|key|
        let value = ($color | get $key)
        if $value <= 0.04045 { $value / 12.92 } else { (($value + 0.055) / 1.055) ** 2.4 }
    })
    ($channels.0 * 0.2126 + $channels.1 * 0.7152 + $channels.2 * 0.0722)
}
def pixel-contrast [one two] {
    let first = (luminance $one)
    let second = (luminance $two)
    (([$first $second] | math max) + 0.05) / (([$first $second] | math min) + 0.05)
}
let repository = ($env.FILE_PWD | path join .. .. | path expand)
let appearance_package = (^nix build ($repository + '#appearance-runtime') --no-link --print-out-paths | lines | last)
let appearance_binary = ($appearance_package | path join bin lumen-appearance)
let sandbox = (^mktemp -d -t lumen-switch-paint.XXXXXX | str trim)
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
    for accent in [blue purple pink red orange yellow green graphite] {
        for contrast in [false true] {
            let resolved = (^$appearance_binary --resolve-palette $mode $accent ($contrast | into string) | from json)
            for checked in [false true] {
                $cases = ($cases | append {mode: $mode, accent: $accent, contrast: $contrast, checked: $checked, colors: $resolved})
            }
        }
    }
}
let qml = ($sandbox | path join shell.qml)
(open --raw ($repository | path join tests desktop switch_indicator_paint.qml) | str replace '__CASES__' ($cases | to json --raw)) | save $qml
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
    LUMEN_SWITCH_CAPTURE_DIR: $sandbox
    LUMEN_APPEARANCE_STATE: ($sandbox | path join appearance.json)
} { ^timeout 30s quickshell -p $qml | complete })
let log = ($run.stdout + $run.stderr)
if $run.exit_code != 0 or ($log | str contains SWITCH_PAINT_FAILURE) or ($log | str contains ReferenceError) or ($log | str contains TypeError) {
    error make {msg: $'Switch QML capture failed: ($log)'}
}
let receipt = ($log | lines | where {|line| $line | str contains 'SWITCH_PAINT_RESULT '})
if ($receipt | length) != 1 { error make {msg: $'Missing switch paint receipt: ($log)'} }
let rows = ($receipt | first | split row 'SWITCH_PAINT_RESULT ' | last | from json)
if ($rows | length) != ($cases | length) { error make {msg: 'Switch paint case count changed'} }
for row in $rows {
    let rect = $row.rect
    let x = (($rect.x * 3) | math round | into int)
    let y = (($rect.y * 3) | math round | into int)
    let width = (($rect.width * 3) | math round | into int)
    let height = (($rect.height * 3) | math round | into int)
    let histogram = (^magick $row.capture -crop $'($width)x($height)+($x)+($y)' +repage -format '%c' histogram:info: | lines)
    mut best = 0.0
    mut best_pixels = 0
    for color in [$row.thumb $row.boundary] {
        mut matching = 0
        mut weakest = 1000.0
        for line in $histogram {
            let parts = ($line | split row ':')
            let parsed = ($line | parse -r 'srgba?\((?<r>\d+),(?<g>\d+),(?<b>\d+)(?:,[^)]+)?\)')
            if ($parsed | is-empty) { continue }
            let pixel = ($parsed | first)
            let actual = {r: (($pixel.r | into int) / 255), g: (($pixel.g | into int) / 255), b: (($pixel.b | into int) / 255)}
            let difference = ([r g b] | each {|key| (((($actual | get $key) - ($color | get $key)) * 255) | math abs)} | math max)
            if $difference <= 16 {
                let count = ($parts.0 | str trim | into int)
                $matching += $count
                let ratio = (pixel-contrast $actual $row.fill)
                if $ratio < $weakest { $weakest = $ratio }
            }
        }
        if $matching >= 5 and $weakest > $best {
            $best = $weakest
            $best_pixels = $matching
        }
    }
    if $best < 3 or $best_pixels < 5 {
        error make {msg: $'Switch boundary failed: ($row.mode), ($row.accent), checked=($row.checked), contrast=($best), pixels=($best_pixels)'}
    }
}
rm --recursive --force $sandbox
print $'PASS (($rows | length)) production switch accent, mode, contrast and on/off paint cases'
